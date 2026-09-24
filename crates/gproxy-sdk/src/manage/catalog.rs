//! The static catalogues: the channels this build compiled in, the bundled
//! model catalog, and the TLS and rewrite presets.
//!
//! None of this is configuration. Everything here is data this binary already
//! holds, offered so a management UI can render a form, and so the two
//! operations that *do* write — [`Catalog::apply_default_prices`] and
//! [`Catalog::apply_rule_preset`] — start from something an operator did not
//! have to type.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::LazyLock,
};

use gproxy_channel::ChannelDescriptor;
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::pricing::{price_rate, price_rule, price_tier, price_unit::PriceUnit};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};

use super::{Rewrite, Scope, Writer, crud};
use crate::{
    SdkError, SdkResult,
    dto::{
        ApplyDefaultPricesReportDto, ApplyDefaultPricesRequest, ApplyRulePreset,
        DefaultModelCatalogDto, DefaultModelDto, DefaultModelPricingDto, DefaultModelTierDto,
        RewriteRuleDto, RewriteRuleWrite, RulePresetCategory, RulePresetDto, TlsPresetDto,
    },
};

/// Prices are quoted in US dollars throughout the bundled catalog; the column
/// is per-rule so an operator's own rules may say otherwise.
const CATALOG_CURRENCY: &str = "USD";

const CATALOG_JSON: &str = include_str!("../../assets/default-model-catalog.json");

/// Parsed once, on the first lookup. The error is kept as a string so the
/// static stays `Send + Sync`; a build whose asset does not parse should fail
/// the same way every time rather than at a random call site.
static CATALOG: LazyLock<Result<DefaultModelCatalogDto, String>> =
    LazyLock::new(|| parse_catalog().map_err(|error| error.to_string()));

fn parse_catalog() -> Result<DefaultModelCatalogDto, String> {
    let catalog: DefaultModelCatalogDto =
        serde_json::from_str(CATALOG_JSON).map_err(|error| error.to_string())?;
    // The asset's own counters are the cheapest possible check that it was not
    // truncated or regenerated against a different shape.
    if catalog.source.total_models != catalog.models.len() {
        return Err(format!(
            "catalog declares {} models and carries {}",
            catalog.source.total_models,
            catalog.models.len()
        ));
    }
    let priced = catalog
        .models
        .iter()
        .filter(|model| model.pricing.is_some())
        .count();
    if catalog.source.priced_models != priced {
        return Err(format!(
            "catalog declares {} priced models and carries {priced}",
            catalog.source.priced_models
        ));
    }
    Ok(catalog)
}

pub struct Catalog<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Catalog<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }

    /// Every compiled-in channel as data, ordered by id. Identical to
    /// `Gproxy::channels()`; it is here too because a management UI reaches
    /// for one catalogue accessor rather than two.
    pub fn channels(&self) -> Vec<ChannelDescriptor> {
        let registry = self.writer.core().channels();
        let mut out: Vec<ChannelDescriptor> = registry
            .ids()
            .filter_map(|id| registry.get(id))
            .map(|channel| channel.descriptor())
            .collect();
        out.sort_by_key(|descriptor| descriptor.id);
        out
    }

    /// The bundled model catalog: names, context windows and default prices
    /// for the models this release knew about. It is a snapshot taken when the
    /// asset was generated, not a live directory — `connectivity().
    /// discover_models` is what asks an upstream what it offers today.
    pub fn default_models(&self) -> SdkResult<&'static DefaultModelCatalogDto> {
        catalog()
    }

    /// The client identities a connection profile's `emulation` can be set to.
    pub fn tls_presets(&self) -> Vec<TlsPresetDto> {
        tls_presets()
    }

    /// The rewrite rule sets this build ships, each one a complete replacement
    /// for a rule set's rules.
    pub fn rule_presets(&self) -> Vec<RulePresetDto> {
        rule_presets()
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Catalog<'_, C> {
    /// Write the catalog's prices for some models, as one revision commit.
    ///
    /// Two scopes, and they behave differently on purpose. Without a provider
    /// the catalog's own `*fragment*` glob and priority are used, so one rule
    /// prices that model wherever it is served. With a provider the literal
    /// name becomes the pattern at priority zero, because a provider-specific
    /// price is about that provider's contract rather than about the model.
    ///
    /// `overwrite: false` is what makes re-applying the catalog safe: a rule
    /// an operator has edited keeps its edit and is reported as skipped.
    pub async fn apply_default_prices(
        &self,
        request: ApplyDefaultPricesRequest,
    ) -> SdkResult<ApplyDefaultPricesReportDto> {
        let catalog = catalog()?;
        let provider_id = match crud::optional_text(request.provider_id) {
            Some(id) => {
                crud::require_rows(
                    self.writer.store().providers(),
                    "provider",
                    std::slice::from_ref(&id),
                )
                .await?;
                Some(id)
            }
            None => None,
        };
        let wanted: BTreeSet<String> = request
            .model_ids
            .iter()
            .map(|id| id.trim().to_owned())
            .collect();
        if wanted.is_empty() || wanted.iter().any(String::is_empty) {
            return Err(SdkError::invalid(
                "modelIds must be a nonempty list of nonblank model names",
            ));
        }

        // Only rules of the same scope collide: a global rule and a provider
        // rule may carry the same pattern and mean different things.
        let existing: BTreeMap<String, String> = self
            .writer
            .store()
            .price_rules()
            .query(price_rule::Entity::find())
            .await?
            .into_iter()
            .filter(|rule| rule.provider_id == provider_id)
            .map(|rule| (rule.model_pattern, rule.id))
            .collect();

        let mut report = ApplyDefaultPricesReportDto::default();
        let mut statements: Vec<BatchStatement> = Vec::new();
        let mut handled: BTreeSet<String> = BTreeSet::new();
        let store = self.writer.store();

        for name in &wanted {
            let Some(source) = (match &provider_id {
                Some(_) => price_for(catalog, name),
                None => model_for(catalog, name).and_then(|model| model.pricing.as_ref()),
            }) else {
                report.unmatched += 1;
                continue;
            };
            let (pattern, priority) = match &provider_id {
                Some(_) => (name.clone(), 0),
                None => (
                    source.model_pattern.clone(),
                    i32::try_from(source.priority).unwrap_or(i32::MAX),
                ),
            };
            // Two requested names can resolve to one catalog glob; the second
            // one has nothing left to write.
            if !handled.insert(pattern.clone()) {
                report.skipped += 1;
                continue;
            }
            let rule_id = match existing.get(&pattern) {
                Some(id) => {
                    if !request.overwrite {
                        report.skipped += 1;
                        continue;
                    }
                    // Rates and tiers are replaced wholesale: a catalog entry
                    // is one price book, and merging halves of two of them
                    // would produce a price nobody published.
                    statements.push(BatchStatement::Execute(
                        store.price_rates().delete_where_statement(
                            price_rate::Entity::delete_many()
                                .filter(price_rate::Column::PriceRuleId.eq(id)),
                        ),
                    ));
                    statements.push(BatchStatement::Execute(
                        store.price_tiers().delete_where_statement(
                            price_tier::Entity::delete_many()
                                .filter(price_tier::Column::PriceRuleId.eq(id)),
                        ),
                    ));
                    let rule = rule_model(id.clone(), provider_id.clone(), pattern, priority);
                    if let Some(statement) = store.price_rules().update_statement(rule)? {
                        statements.push(BatchStatement::Execute(statement));
                    }
                    report.updated += 1;
                    id.clone()
                }
                None => {
                    let id = crate::ids::random_id();
                    let rule = rule_model(id.clone(), provider_id.clone(), pattern, priority);
                    statements.push(BatchStatement::Execute(
                        store.price_rules().insert_statement(rule)?,
                    ));
                    report.created += 1;
                    id
                }
            };
            for rate in &source.rates {
                statements.push(BatchStatement::Execute(
                    store
                        .price_rates()
                        .insert_statement(rate_model(&rule_id, rate)?)?,
                ));
            }
            for tier in source.tiers.iter().flatten() {
                statements.push(BatchStatement::Execute(
                    store
                        .price_tiers()
                        .insert_statement(tier_model(&rule_id, tier)?)?,
                ));
            }
        }

        if !statements.is_empty() {
            self.writer.commit(statements, &[Scope::Pricing]).await?;
        }
        Ok(report)
    }

    /// Replace one rule set's rules with a preset's.
    ///
    /// A replacement rather than a merge: a preset is a complete, ordered
    /// answer to "make this client look like a generic one", and half of it
    /// interleaved with something else rewrites text nobody predicted. A
    /// caller that wants to keep existing rules reads `rule_presets()` and
    /// sends its own list to `rewrite().replace_rules`.
    pub async fn apply_rule_preset(
        &self,
        request: ApplyRulePreset,
    ) -> SdkResult<Vec<RewriteRuleDto>> {
        let preset = rule_presets()
            .into_iter()
            .find(|preset| preset.id == request.preset_id.trim())
            .ok_or_else(|| SdkError::not_found("rule preset", request.preset_id.clone()))?;
        Rewrite::new(self.writer)
            .replace_rules(request.rule_set_id.trim(), preset.rules)
            .await
    }
}

fn catalog() -> SdkResult<&'static DefaultModelCatalogDto> {
    CATALOG.as_ref().map_err(|error| {
        SdkError::invalid(format!("the bundled model catalog is unusable: {error}"))
    })
}

/// A catalog entry by name: an exact `vendor/model` first, then the bare model
/// name when exactly one vendor offers it. An ambiguous bare name resolves to
/// nothing rather than to whichever vendor happens to be first.
fn model_for<'a>(catalog: &'a DefaultModelCatalogDto, name: &str) -> Option<&'a DefaultModelDto> {
    let needle = name.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    if let Some(found) = catalog
        .models
        .iter()
        .find(|model| model.model_id.to_ascii_lowercase() == needle)
    {
        return Some(found);
    }
    let basename = |value: &str| {
        value
            .rsplit_once('/')
            .map_or(value, |(_, tail)| tail)
            .to_ascii_lowercase()
    };
    let needle = basename(&needle);
    let mut matches = catalog
        .models
        .iter()
        .filter(|model| basename(&model.model_id) == needle);
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

/// The price book whose `*fragment*` covers `name`, preferring the longest
/// fragment. That is the same ordering `priority` encodes, so the rule this
/// picks is the rule that would have won at settlement.
fn price_for<'a>(
    catalog: &'a DefaultModelCatalogDto,
    name: &str,
) -> Option<&'a DefaultModelPricingDto> {
    let needle = name.trim().to_ascii_lowercase();
    catalog
        .models
        .iter()
        .filter_map(|model| model.pricing.as_ref())
        .filter_map(|pricing| {
            let fragment = pricing
                .model_pattern
                .trim_start_matches('*')
                .trim_end_matches('*')
                .to_ascii_lowercase();
            needle
                .contains(&fragment)
                .then_some((fragment.len(), pricing))
        })
        .fold(None, |best: Option<(usize, _)>, candidate| match best {
            Some(best) if best.0 >= candidate.0 => Some(best),
            _ => Some(candidate),
        })
        .map(|(_, pricing)| pricing)
}

/// Whether the bundled catalog can price a name without an operator writing a
/// rule. Used by model discovery to mark what is ready to use.
pub(crate) fn has_default_price(name: &str) -> bool {
    catalog().is_ok_and(|catalog| price_for(catalog, name).is_some())
}

fn rule_model(
    id: String,
    provider_id: Option<String>,
    model_pattern: String,
    priority: i32,
) -> price_rule::ActiveModel {
    price_rule::ActiveModel {
        id: Set(id),
        provider_id: Set(provider_id),
        model_pattern: Set(model_pattern),
        // The catalog prices a model, not one of its operations.
        operation: Set(None),
        priority: Set(priority),
        currency: Set(CATALOG_CURRENCY.to_owned()),
        enabled: Set(true),
    }
}

fn rate_model(
    rule_id: &str,
    rate: &crate::dto::DefaultModelPriceRateDto,
) -> SdkResult<price_rate::ActiveModel> {
    Ok(price_rate::ActiveModel {
        id: Set(crate::ids::random_id()),
        price_rule_id: Set(rule_id.to_owned()),
        metric: Set(rate.metric.clone()),
        // Token metrics are quoted per million; everything else the catalog
        // carries is an event count.
        unit: Set(match rate.metric.ends_with("_tokens") {
            true => PriceUnit::Token,
            false => PriceUnit::Count,
        }),
        unit_quantity: Set(crud::decimal(&rate.unit_size.to_string(), "unitSize")?),
        value: Set(crud::decimal(&rate.price, "price")?),
        conditions: Set(None),
        priority: Set(i32::try_from(rate.priority).unwrap_or(i32::MAX)),
    })
}

fn tier_model(rule_id: &str, tier: &DefaultModelTierDto) -> SdkResult<price_tier::ActiveModel> {
    let money = |value: &Option<String>, field: &'static str| match value {
        Some(value) => crud::decimal(value, field).map(Some),
        None => Ok(None),
    };
    Ok(price_tier::ActiveModel {
        id: Set(crate::ids::random_id()),
        price_rule_id: Set(rule_id.to_owned()),
        service_tier: Set(tier.service_tier.clone()),
        min_prompt_tokens: Set(tier.min_prompt_tokens.unwrap_or(0)),
        priority: Set(0),
        multiplier: Set(money(&tier.multiplier, "multiplier")?),
        input_per_million: Set(money(&tier.input_price, "inputPrice")?),
        output_per_million: Set(money(&tier.output_price, "outputPrice")?),
        cache_read_per_million: Set(money(&tier.cache_read_price, "cacheReadPrice")?),
        cache_creation_5m_per_million: Set(money(
            &tier.cache_creation_5m_price,
            "cacheCreation5mPrice",
        )?),
        cache_creation_30m_per_million: Set(money(
            &tier.cache_creation_30m_price,
            "cacheCreation30mPrice",
        )?),
        cache_creation_1h_per_million: Set(money(
            &tier.cache_creation_1h_price,
            "cacheCreation1hPrice",
        )?),
        image_output_per_million: Set(money(&tier.image_output_price, "imageOutputPrice")?),
        ..Default::default()
    })
}

// ---------------------------------------------------------------------------
// TLS presets.
// ---------------------------------------------------------------------------

/// The legacy CLI fingerprints, followed by the named emulations supported
/// by the compiled wreq backend. Browser profiles come from wreq-util itself
/// rather than a separate version list that can drift from the client.
fn tls_presets() -> Vec<TlsPresetDto> {
    let simple = |id: &str, label: &str, user_agent: &str, curves: &str| TlsPresetDto {
        id: id.to_owned(),
        label: label.to_owned(),
        emulation: json!({
            "kind": "custom",
            "alpn": [],
            "min_tls": "tls12",
            "max_tls": "tls13",
            "grease": false,
            "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
            "curves_list": curves,
            "headers": [["user-agent", user_agent]],
        }),
    };
    let mut presets = vec![
        TlsPresetDto {
            id: "claude".to_owned(),
            label: "Claude CLI".to_owned(),
            emulation: json!({
                "kind": "custom",
                "alpn": ["http1"],
                "min_tls": "tls12",
                "max_tls": "tls13",
                "grease": false,
                "cipher_list": "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256",
                "curves_list": "X25519:P-256:P-384",
                "headers": [["user-agent", "claude-cli/2.1.112 (external, cli)"]],
            }),
        },
        TlsPresetDto {
            id: "codex".to_owned(),
            label: "Codex CLI".to_owned(),
            emulation: json!({
                "kind": "custom",
                "alpn": ["http2"],
                "min_tls": "tls12",
                "max_tls": "tls13",
                "grease": false,
                "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
                "curves_list": "X25519:P-256:P-384",
                "http2": {
                    "enable_push": false,
                    "initial_window_size": 2097152,
                    "initial_connection_window_size": 5242880,
                    "max_frame_size": 16384,
                    "max_header_list_size": 16384,
                    "pseudo_header_order": ["method", "scheme", "authority", "path"],
                    // The wire ids v3 listed, by name: 2, 4, 5, 6.
                    "settings_order": [
                        "enable_push",
                        "initial_window_size",
                        "max_frame_size",
                        "max_header_list_size",
                    ],
                },
                "headers": [
                    ["user-agent", "codex_exec/0.144.0 (Debian 13.0.0; x86_64) xterm-256color"],
                    ["originator", "codex_exec"],
                ],
            }),
        },
        simple(
            "gemini",
            "Gemini CLI",
            "google-api-nodejs-client/9.15.1",
            "X25519MLKEM768:X25519:P-256:P-384:P-521",
        ),
        simple(
            "antigravity",
            "Antigravity",
            "codeium-language-server",
            "X25519MLKEM768:X25519:P-256:P-384:P-521",
        ),
        simple(
            "kiro",
            "Kiro CLI",
            "aws-sdk-rust/1.3.10 os/linux lang/rust/1.92.0",
            "X25519:P-256:P-384",
        ),
        simple(
            "copilot",
            "GitHub Copilot CLI",
            "copilot/1.0.61 (linux v24.16.0) term/unknown",
            "X25519:P-256:P-384",
        ),
    ];
    for profile in gproxy_client::emulation_profiles() {
        let (family, version) = profile.split_once('_').unwrap_or((&profile, ""));
        let family = match family {
            "chrome" => "Chrome",
            "edge" => "Edge",
            "firefox" => "Firefox",
            "safari" => "Safari",
            "opera" => "Opera",
            "okhttp" => "OkHttp",
            other => other,
        };
        let platform = if profile.starts_with("safari_ios") || profile.starts_with("safari_ipad") {
            "ios"
        } else if profile.starts_with("safari_") {
            "macos"
        } else if profile.starts_with("okhttp_") {
            "android"
        } else {
            match std::env::consts::OS {
                "windows" => "windows",
                "macos" => "macos",
                _ => "linux",
            }
        };
        presets.push(TlsPresetDto {
            id: format!("wreq:{profile}"), label: format!("{family} {}", version.replace('_', " ")),
            emulation: json!({"kind":"preset", "profile":profile, "platform":platform, "http2":true, "headers":true}),
        });
    }
    presets
}

// ---------------------------------------------------------------------------
// Rewrite rule presets.
// ---------------------------------------------------------------------------

/// Where a generation request keeps text an upstream reads as instructions,
/// across the three wire families. A rewrite that names all of them works
/// whichever dialect the caller speaks.
const REQUEST_TEXT_PATHS: &[&str] = &[
    "system",
    "system.*.text",
    "instructions",
    "messages.*.content",
    "messages.*.content.*.text",
    "messages.*.content.*.content.*.text",
    "input",
    "input.*.content",
    "input.*.content.*.text",
    "contents.*.parts.*.text",
    "systemInstruction.parts.*.text",
];

/// Where a request names a tool.
const REQUEST_TOOL_PATHS: &[&str] = &[
    "tools.*.name",
    "tool_choice.name",
    "messages.*.content.*.name",
    "messages.*.content.*.tool_name",
    "messages.*.content.*.content.*.tool_name",
];

/// Where a response names a tool, including the streaming block forms.
const RESPONSE_TOOL_PATHS: &[&str] = &[
    "content.*.name",
    "content.*.tool_name",
    "message.content.*.name",
    "message.content.*.tool_name",
    "content_block.name",
    "content_block.tool_name",
];

/// The OpenCode tool vocabulary, lower case upstream, capitalized downstream.
const TOOL_RENAMES: &[(&str, &str)] = &[
    ("bash", "Bash"),
    ("read", "Read"),
    ("write", "Write"),
    ("edit", "Edit"),
    ("glob", "Glob"),
    ("grep", "Grep"),
    ("task", "Task"),
    ("webfetch", "WebFetch"),
    ("todowrite", "TodoWrite"),
    ("question", "Question"),
    ("skill", "Skill"),
    ("ls", "LS"),
    ("todoread", "TodoRead"),
    ("notebookedit", "NotebookEdit"),
];

/// Generation only, across the four HTTP dialects. Every preset path below is
/// a generation-request or generation-response shape, so a rule that ran on a
/// model listing or a file upload would only ever waste work.
fn generation_keys() -> Value {
    let mut keys = Vec::new();
    for operation in ["generate_content", "stream_generate_content"] {
        for dialect in ["openai", "openai_chat", "claude", "gemini"] {
            keys.push(json!({ "operation": operation, "dialect": dialect }));
        }
    }
    Value::Array(keys)
}

fn paths(values: &[&str]) -> Value {
    Value::Array(values.iter().map(|path| json!(path)).collect())
}

/// One body rewrite of a preset. Everything a preset rule has in common lives
/// here: the request phase, the body target, the generation filter and the
/// client header that scopes it.
fn rule(
    sort_order: i64,
    phase: &str,
    selected: &[&str],
    pattern: &str,
    replacement: &str,
    header: Option<&str>,
) -> RewriteRuleWrite {
    RewriteRuleWrite {
        phase: Some(phase.to_owned()),
        target: Some("body".to_owned()),
        paths: Some(paths(selected)),
        pattern: pattern.to_owned(),
        replacement: replacement.to_owned(),
        filter_operation_keys: Some(generation_keys()),
        filter_header_pattern: header.map(str::to_owned),
        sort_order: Some(sort_order),
        enabled: Some(true),
        ..Default::default()
    }
}

/// A preset that only renames one client in the text it sends.
fn sanitize(
    id: &str,
    name: &str,
    header: Option<&str>,
    replacements: &[(&str, &str)],
) -> RulePresetDto {
    RulePresetDto {
        id: id.to_owned(),
        name: name.to_owned(),
        description: format!("gproxy:preset:{id}:v1"),
        category: RulePresetCategory::Application,
        rules: replacements
            .iter()
            .enumerate()
            .map(|(index, (pattern, replacement))| {
                rule(
                    index as i64,
                    "request",
                    REQUEST_TEXT_PATHS,
                    pattern,
                    replacement,
                    header,
                )
            })
            .collect(),
    }
}

fn rule_presets() -> Vec<RulePresetDto> {
    vec![
        opencode(),
        sanitize(
            "the agent",
            "the agent-mono",
            None,
            &[
                (r"\bPi documentation\b", "Harness documentation"),
                (r"\binside the agent, a coding\b", "inside the coding"),
                (r"\bpi packages\b", "harness packages"),
                (r"\bpi topics\b", "harness topics"),
                (r"\bpi \.md files\b", "the harness .md files"),
                (r"\bpi itself\b", "the harness itself"),
                (r"\bpi\b", "the agent"),
                (r"\bPi\b", "The agent"),
                (r"\bPI\b", "AGENT"),
            ],
        ),
        sanitize(
            "aider",
            "Aider",
            Some("^user-agent: litellm/"),
            &[
                (r"\bAider\b", "The assistant"),
                (r"\baider\b", "the assistant"),
            ],
        ),
        sanitize(
            "cline",
            "Cline",
            Some(r"^user-agent: cline/|^x-title: cline$|^http-referer: https://cline\.bot"),
            &[(r"\bCline\b", "Assistant")],
        ),
        sanitize(
            "continue",
            "Continue",
            Some("^user-agent: continue/"),
            &[(r"\bContinue\b", "Assistant")],
        ),
        sanitize(
            "cursor",
            "Cursor",
            Some("^user-agent: cursor/"),
            &[(r"\bCursor\b", "Assistant")],
        ),
    ]
}

/// OpenCode needs more than a rename: it also renames its tools on the way
/// out and back, so the model sees the tool vocabulary it was trained on while
/// the client keeps the names it dispatches on.
fn opencode() -> RulePresetDto {
    const CLIENT: &str = "^user-agent: opencode/";
    let mut rules = Vec::new();
    let mut order = 0i64;
    let mut push = |phase: &str, selected: &[&str], pattern: &str, replacement: &str| {
        rules.push(rule(
            order,
            phase,
            selected,
            pattern,
            replacement,
            Some(CLIENT),
        ));
        order += 1;
    };
    for (pattern, replacement) in [
        (
            r"(?s)Here is some useful information about the environment you are running in:\s*<env>.*?</env>\n?",
            "",
        ),
        (
            r"(?i)https://github\.com/anomalyco/opencode(?:/[^\s)]*)?",
            "the project issue tracker",
        ),
        (
            r"(?i)https://opencode\.ai/docs(?:/[^\s)]*)?",
            "the documentation",
        ),
        (
            r"(?i)(?:~/)?\.config/opencode/|\.opencode/",
            "the assistant config directory/",
        ),
        (r"(?i)/tmp/opencode\b", "/tmp/coding-agent"),
        (r"(?i)\bopencode\b", "the coding assistant"),
        (r"\bgit repo\b", "git repository"),
    ] {
        push(
            "request",
            &["system", "system.*.text"],
            pattern,
            replacement,
        );
    }
    // A tool rename is a whole-value replacement, not a substring one: `^…$`
    // is what keeps `read` from rewriting the middle of `todoread`.
    for (from, to) in TOOL_RENAMES {
        push("request", REQUEST_TOOL_PATHS, &format!("^{from}$"), to);
    }
    push("request", REQUEST_TOOL_PATHS, r"^mcp_([^_].*)$", "mcp__$1");
    for (from, to) in TOOL_RENAMES {
        push("response", RESPONSE_TOOL_PATHS, &format!("^{to}$"), from);
    }
    push(
        "response",
        RESPONSE_TOOL_PATHS,
        r"^mcp__([^_].*)$",
        "mcp_$1",
    );
    RulePresetDto {
        id: "opencode".to_owned(),
        name: "OpenCode".to_owned(),
        description: "gproxy:preset:opencode:v1".to_owned(),
        category: RulePresetCategory::Application,
        rules,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A preset that does not deserialize into the client's own type would be
    /// stored happily and then fail at assembly, one provider at a time. The
    /// only place to catch that is here.
    #[test]
    fn tls_presets_are_client_emulations() {
        for preset in tls_presets() {
            let parsed: Result<gproxy_client::EmulationConfig, _> =
                serde_json::from_value(preset.emulation.clone());
            assert!(
                parsed.is_ok(),
                "preset `{}` is not an emulation: {:?}",
                preset.id,
                parsed.err()
            );
        }
    }

    /// The asset's own counters, and that a well-known entry is reachable by
    /// both lookups.
    #[test]
    fn bundled_catalog_parses() {
        let catalog = catalog().expect("the bundled catalog parses");
        assert_eq!(catalog.models.len(), catalog.source.total_models);
        assert!(catalog.source.priced_models > 400);
        let found = model_for(catalog, "claude-sonnet-4").expect("a bare name resolves");
        assert_eq!(found.model_id, "anthropic/claude-sonnet-4");
        let pricing = price_for(catalog, "anthropic/claude-sonnet-4").expect("a glob covers it");
        assert!(pricing.model_pattern.contains("claude-sonnet-4"));
        assert!(
            pricing
                .rates
                .iter()
                .any(|rate| rate.metric == "input_tokens")
        );
    }
}
