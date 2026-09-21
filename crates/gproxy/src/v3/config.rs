//! The configuration half: a v3 document as a v4 [`ConfigurationExportDto`].
//!
//! Nothing here writes to a database. It produces the same document
//! `gproxy export` writes, which [`crate::transfer`] already knows how to
//! replay — so the import runs through the sdk's one transaction, its one
//! revision bump, its reference checking and its credential re-sealing, and
//! this module's only job is the translation.
//!
//! # What maps, and how
//!
//! | v3 | v4 | |
//! |---|---|---|
//! | `providers` | `providers` | direct; `settings` becomes `config`, and a `base_url` inside it is lifted into v4's column |
//! | `providers.proxy_url`, `credentials.proxy_url` | `connection_profiles` | one profile per distinct URL, `proxy_mode = explicit` |
//! | `credentials` | `credentials` | direct; `kind` is v4's `auth_kind`; the secret is opened and re-sealed |
//! | `credentials.rpm_limit` | `quotas` | an operator limit: `requests` per 60 seconds on that credential |
//! | `routes`, `route_members` | same | direct |
//! | `model_aliases` | `exposed_models` | direct; v3 exported the table under the console's name for it |
//! | `provider_models` | `provider_models` | direct; v3's `model_id` is v4's `upstream_name` |
//! | `price_rules` | `price_rules` | direct; v3 priced in USD only, so the currency is `USD` |
//! | `price_rules.tiers` | `price_tiers` | the JSON array becomes rows, field for field |
//! | `price_rates` | `price_rates` | direct; `unit_size` is `unit_quantity`, and the unit is read off the metric name |
//! | `quotas` | `quotas` | one v3 row is up to six v4 rows, one per period column |
//! | `rule_sets`, `provider_rule_sets` | `rewrite_rule_sets`, `provider_rewrite_rule_sets` | direct |
//! | `rules` (`transform` only) | `rewrite_rules` | one v4 rule per v3 action; see below |
//!
//! # What does not map, and why
//!
//! - **`aliases`.** v3 could say "when a request names `gpt4`, send
//!   `gpt-4-turbo`", globally or for one provider. v4 has no such table: a
//!   public name is an `exposed_models` row pointing at a route, and the
//!   upstream name is the route member's. There is no faithful automatic
//!   rewrite, because one v3 alias is a route plus a member plus an exposed
//!   name, and which providers should serve it is a decision v3 never recorded.
//! - **`routing_rules`.** They look like v4's `operation_rules` and are not.
//!   v3 **seeded** them from the channel's own defaults whenever a provider was
//!   created (`v3:crates/gproxy-admin/src/defaults.rs`), so the table is mostly
//!   a frozen copy of v3's channel declarations. v4 asks the channel at
//!   assembly instead and only stores a row to *override* it. Importing them
//!   would pin every provider to what v3's channels supported on the day the
//!   provider was made, which is the opposite of an upgrade.
//! - **`rules` other than `transform`.** v4's rewrite rules are regex
//!   replacements over a body path, a header value or a query parameter. v3's
//!   `system_text`, `cache_breakpoint`, `rewrite` (JSON set/delete/merge) and
//!   `header` (set/merge a header) are *constructions*, not replacements, and
//!   nothing in v4 performs them.
//! - **`credentials.weight` and `credentials.tpm_limit`.** v4 balances by route
//!   member weight, and its credential limits meter requests and cost, not
//!   tokens (`gproxy_core::credential_limit`).
//! - **`quotas` on a `user_key` whose key did not survive**, and anything else
//!   whose owner is gone: reported rather than written against a dangling id.
//!
//! Every one of those is named row by row in the [`Report`], not summarized.

use std::collections::BTreeMap;

use gproxy_sdk::dto::{
    ConfigurationDataDto, ConfigurationExportDto, ConnectionProfileDto, CredentialDto,
    EXPORT_FORMAT_VERSION, ExportCredentialDto, ExposedModelDto, ModelDto, PriceRateDto,
    PriceRuleDto, PriceTierDto, ProviderDto, ProviderModelDto, ProviderRuleSetDto, QuotaDto,
    RewriteRuleDto, RouteDto, RouteMemberDto, RuleSetDto, SealedSecretDto,
};
use serde_json::Value;

use super::{
    Report,
    document::{self, Action, Document, Locate, RuleConfig},
    ids,
    secret::{Bridge, Domain},
};
use crate::{Error, Result};

/// v3 had one currency and never stored it.
const CURRENCY: &str = "USD";

/// v3's `rpm_limit` is a per-minute request ceiling; v4 spells a window in
/// seconds, and `1m` would be a calendar month.
const RPM_PERIOD_SECONDS: i64 = 60;

/// What one translation produced: the document to hand the sdk, and the key it
/// needs to open the secrets inside it.
#[derive(Debug)]
pub struct Configuration {
    pub export: ConfigurationExportDto,
    pub report: Report,
}

/// Translate the configuration half. `bridge` opens v3's sealed credentials and
/// re-seals them for the sdk; `timestamp` is what every `created_at_ms` column
/// v3 never had becomes.
pub fn translate(document: &Document, bridge: &Bridge, timestamp: i64) -> Result<Configuration> {
    let mut report = Report::default();
    let data = &document.data;

    let profiles = Profiles::collect(data);
    let credentials = credentials(data, bridge, &profiles, &mut report)?;
    let (price_rules, price_tiers) = price_rules(data, &mut report);
    let (rewrite_rules, rule_sets) = rewrite_rules(data, &mut report);

    report.count("connection_profiles", profiles.rows.len() as u64);
    report.count("credentials", credentials.len() as u64);
    report.count("price_rules", price_rules.len() as u64);
    report.count("price_tiers", price_tiers.len() as u64);
    report.count("rewrite_rules", rewrite_rules.len() as u64);
    report.count("rewrite_rule_sets", rule_sets.len() as u64);

    let providers: Vec<ProviderDto> = data
        .providers
        .iter()
        .map(|row| provider(row, &profiles, timestamp))
        .collect();
    report.count("providers", providers.len() as u64);

    let provider_models: Vec<ProviderModelDto> =
        data.provider_models.iter().map(provider_model).collect();
    report.count("provider_models", provider_models.len() as u64);

    let routes: Vec<RouteDto> = data.routes.iter().map(route).collect();
    report.count("routes", routes.len() as u64);
    let route_members: Vec<RouteMemberDto> = data.route_members.iter().map(route_member).collect();
    report.count("route_members", route_members.len() as u64);
    let exposed_models: Vec<ExposedModelDto> =
        data.model_aliases.iter().map(exposed_model).collect();
    report.count("exposed_models", exposed_models.len() as u64);

    let price_rates: Vec<PriceRateDto> = data.price_rates.iter().map(price_rate).collect();
    report.count("price_rates", price_rates.len() as u64);

    let provider_rewrite_rule_sets: Vec<ProviderRuleSetDto> = data
        .provider_rule_sets
        .iter()
        .map(|row| provider_rule_set(row, timestamp))
        .collect();
    report.count(
        "provider_rewrite_rule_sets",
        provider_rewrite_rule_sets.len() as u64,
    );

    let mut quotas = quotas(data, &mut report);
    quotas.extend(credential_limits(data, &mut report));
    report.count("quotas", quotas.len() as u64);

    refuse_unmappable(data, &mut report);

    Ok(Configuration {
        export: ConfigurationExportDto {
            format_version: EXPORT_FORMAT_VERSION,
            exported_at_ms: timestamp,
            secrets_omitted: false,
            // Everything this module emits is sealed by the bridge's own
            // ephemeral key, so there is exactly one codec in the document.
            secrets: vec![gproxy_sdk::dto::CODEC_AES_GCM.to_owned()],
            data: ConfigurationDataDto {
                connection_profiles: profiles.rows,
                providers,
                credentials,
                // v3 had no shared model catalog: a model existed per provider.
                models: Vec::<ModelDto>::new(),
                provider_models,
                routes,
                route_members,
                exposed_models,
                // See the module note: v3's routing rules are channel defaults.
                operation_rules: Vec::new(),
                operation_endpoints: Vec::new(),
                rewrite_rule_sets: rule_sets,
                rewrite_rules,
                provider_rewrite_rule_sets,
                quotas,
                price_rules,
                price_rates,
                price_tiers,
                // v3's export carries no settings row, so the destination keeps
                // the one its own configuration made.
                settings: None,
            },
        },
        report,
    })
}

// ------------------------------------------------------------- providers --

/// v3 put a proxy URL on a provider and on a credential; v4 puts the whole
/// outbound client configuration in a `connection_profiles` row and points at
/// it. One profile per distinct URL, named from the URL so a re-run mints the
/// same ids and an operator can tell which is which.
struct Profiles {
    rows: Vec<ConnectionProfileDto>,
    /// proxy URL → profile id.
    by_url: BTreeMap<String, String>,
}

impl Profiles {
    fn collect(data: &document::Data) -> Self {
        let mut urls: Vec<String> = data
            .providers
            .iter()
            .filter_map(|row| row.proxy_url.as_deref())
            .chain(
                data.credentials
                    .iter()
                    .filter_map(|row| row.config.proxy_url.as_deref()),
            )
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
            .collect();
        // Sorted and deduplicated, so the index a profile gets depends on the
        // document and not on the order this happened to walk it.
        urls.sort();
        urls.dedup();

        let mut rows = Vec::with_capacity(urls.len());
        let mut by_url = BTreeMap::new();
        for (index, url) in urls.into_iter().enumerate() {
            let id = ids::part("connection_profiles", index as i64, "proxy");
            rows.push(ConnectionProfileDto {
                id: id.clone(),
                name: format!("v3 proxy {}", index + 1),
                // v3 had no client backend choice: it used one HTTP client.
                backend: "reqwest".into(),
                proxy_mode: "explicit".into(),
                proxy_url: Some(url.clone()),
                emulation: None,
                gzip: true,
                brotli: true,
                deflate: true,
                zstd: true,
                redirect_max_hops: 0,
                retry: "default".into(),
                connect_timeout_ms: 0,
                pool_idle_timeout_ms: 0,
                pool_max_idle_per_host: 0,
                created_at_ms: 0,
            });
            by_url.insert(url, id);
        }
        Self { rows, by_url }
    }

    fn id(&self, url: Option<&String>) -> Option<String> {
        let url = url.map(|url| url.trim())?;
        self.by_url.get(url).cloned()
    }
}

fn provider(row: &document::Provider, profiles: &Profiles, timestamp: i64) -> ProviderDto {
    // v3 kept the upstream origin inside `settings_json`; v4 has a column for
    // it and its channels read the column. The key stays in `config` as well:
    // removing it would change what a channel that still reads it sees.
    let base_url = row
        .settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned);
    ProviderDto {
        id: ids::id("providers", row.id),
        // v3 had a `name` and a display `label`; v4 has one name, and the
        // label is the one a person chose.
        name: row
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or(row.name.as_str())
            .to_owned(),
        channel: row.channel.clone(),
        base_url,
        connection_profile_id: profiles.id(row.proxy_url.as_ref()),
        config: object(&row.settings),
        enabled: row.enabled,
        created_at_ms: timestamp,
    }
}

/// v4 requires `config` to be an object; v3 stored whatever the channel put
/// there, and an older row can hold `null`.
fn object(value: &Value) -> Value {
    match value {
        Value::Object(_) => value.clone(),
        _ => Value::Object(serde_json::Map::new()),
    }
}

// ----------------------------------------------------------- credentials --

fn credentials(
    data: &document::Data,
    bridge: &Bridge,
    profiles: &Profiles,
    report: &mut Report,
) -> Result<Vec<ExportCredentialDto>> {
    use base64::Engine;
    let mut out = Vec::with_capacity(data.credentials.len());
    for row in &data.credentials {
        let id = ids::id("credentials", row.config.id);
        let Some(envelope) = &row.secret else {
            // Not a warning: core opens every credential secret while it
            // assembles a snapshot, so a credential without one is not a
            // degraded row, it is a row that breaks every reload.
            return Err(Error::other(format!(
                "credential {} ({}) carries no secret. Take the v3 export again with \
                 {{\"include_secrets\": true}}; a configuration-only export cannot create a \
                 credential on the destination.",
                row.config.id,
                row.config.label.as_deref().unwrap_or("unlabelled")
            )));
        };
        let opened = bridge.open(Domain::Credential, &id, envelope)?;
        let sealed = bridge.seal_for_sdk(&id, &opened)?;

        if row.config.weight != 0 && row.config.weight != 100 {
            report.warn(format!(
                "credential {} had weight {}: v4 balances by route member weight, not per \
                 credential, so the weighting was not carried",
                row.config.id, row.config.weight
            ));
        }
        if row.config.tpm_limit.is_some() {
            report.drop_row(
                "credentials.tpm_limit",
                format!("credential {}", row.config.id),
                "v4 meters credential limits in requests and cost, not tokens",
            );
        }

        out.push(ExportCredentialDto {
            credential: CredentialDto {
                id: id.clone(),
                provider_id: ids::id("providers", row.config.provider_id),
                // v3 credentials belonged to the instance, never to an
                // organization, a team or a user.
                organization_id: None,
                team_id: None,
                user_id: None,
                label: row.config.label.clone(),
                auth_kind: row.config.kind.clone(),
                has_secret: true,
                // v4 bumps this on every secret write; starting from v3's value
                // keeps a peer's cached credential from looking newer than the
                // row it was replaced by.
                version: i64::try_from(row.config.version).unwrap_or(1).max(1),
                connection_profile_id: profiles.id(row.config.proxy_url.as_ref()),
                metadata: Value::Object(serde_json::Map::new()),
                // v3's credentials had no expiry column; an OAuth credential's
                // expiry lived inside the secret and v4's refresh re-reads it.
                expires_at_ms: None,
                status: "active".into(),
                status_reason: None,
                enabled: row.config.enabled,
            },
            secret: Some(SealedSecretDto {
                codec: gproxy_sdk::dto::CODEC_AES_GCM.to_owned(),
                bytes: base64::engine::general_purpose::STANDARD.encode(&sealed),
            }),
        });
    }
    Ok(out)
}

// --------------------------------------------------------------- routing --

fn route(row: &document::Route) -> RouteDto {
    RouteDto {
        id: ids::id("routes", row.id),
        name: row.name.clone(),
        strategy: match row.strategy.as_deref() {
            Some("round_robin") => "round_robin",
            Some("failover") => "failover",
            // v3's column was added late and defaulted to weighted for every
            // row that predated it.
            _ => "weighted",
        }
        .to_owned(),
        max_attempts: row.max_attempts.max(1),
        enabled: row.enabled,
    }
}

fn route_member(row: &document::RouteMember) -> RouteMemberDto {
    RouteMemberDto {
        id: ids::id("route_members", row.id),
        route_id: ids::id("routes", row.route_id),
        provider_id: ids::id("providers", row.provider_id),
        upstream_model: row.upstream_model.clone(),
        tier: row.tier,
        weight: row.weight.max(1),
        enabled: row.enabled,
    }
}

fn exposed_model(row: &document::ModelAlias) -> ExposedModelDto {
    ExposedModelDto {
        id: ids::id("model_aliases", row.id),
        name: row.name.clone(),
        route_id: ids::id("routes", row.route_id),
        enabled: row.enabled,
    }
}

fn provider_model(row: &document::ProviderModel) -> ProviderModelDto {
    let mut metadata = object(&row.metadata);
    // v3 kept these beside the metadata blob rather than inside it; v4 has one
    // metadata object, so they move in rather than being lost.
    if let Value::Object(map) = &mut metadata {
        if let Some(window) = row.context_window {
            map.entry("max_context_window")
                .or_insert_with(|| Value::from(window));
        }
        if let Some(tokens) = row.max_output_tokens {
            map.entry("max_output_tokens")
                .or_insert_with(|| Value::from(tokens));
        }
        if let Some(name) = row.display_name.as_deref().filter(|name| !name.is_empty()) {
            map.entry("display_name")
                .or_insert_with(|| Value::from(name));
        }
    }
    ProviderModelDto {
        id: ids::id("provider_models", row.id),
        provider_id: ids::id("providers", row.provider_id),
        // v3's `model_id` was the upstream's own name for the model.
        upstream_name: row.model_id.clone(),
        // v4's shared catalog is opt-in and v3 had nothing to fill it from.
        model_id: None,
        metadata,
        enabled: row.enabled,
    }
}

// --------------------------------------------------------------- pricing --

fn price_rules(
    data: &document::Data,
    report: &mut Report,
) -> (Vec<PriceRuleDto>, Vec<PriceTierDto>) {
    let mut rules = Vec::with_capacity(data.price_rules.len());
    let mut tiers = Vec::new();
    for row in &data.price_rules {
        let id = ids::id("price_rules", row.id);
        rules.push(PriceRuleDto {
            id: id.clone(),
            provider_id: row.provider_id.map(|id| ids::id("providers", id)),
            model_pattern: row.model_pattern.clone(),
            // v3 priced a model, never one of its operations.
            operation: None,
            priority: i32::try_from(row.priority).unwrap_or(i32::MAX),
            currency: CURRENCY.to_owned(),
            enabled: row.enabled,
        });
        tiers.extend(price_tiers(&id, row, report));
    }
    (rules, tiers)
}

/// v3 kept its tiers as a JSON array on the rule; v4 gives each one a row. The
/// fields line up one for one — v3's `input` is v4's `input_per_million` and so
/// on — because v4's table was built from v3's blob.
fn price_tiers(
    rule_id: &str,
    row: &document::PriceRule,
    report: &mut Report,
) -> Vec<PriceTierDto> {
    let Some(Value::Array(entries)) = row.tiers.as_ref() else {
        if row.tiers.is_some() {
            report.drop_row(
                "price_rules.tiers",
                format!("price rule {}", row.id),
                "the tier column is not a JSON array; v4 stores tiers as rows and cannot read it",
            );
        }
        return Vec::new();
    };
    let money = |entry: &Value, key: &str| -> Option<String> {
        let value = entry.get(key)?;
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| PriceTierDto {
            id: ids::part("price_rules", row.id, &format!("tier-{index}")),
            price_rule_id: rule_id.to_owned(),
            service_tier: entry
                .get("service_tier")
                .and_then(Value::as_str)
                .map(str::to_owned),
            min_prompt_tokens: entry
                .get("min_prompt_tokens")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
            priority: index as i32,
            multiplier: money(entry, "multiplier"),
            input_per_million: money(entry, "input"),
            output_per_million: money(entry, "output"),
            cache_read_per_million: money(entry, "cache_read"),
            cache_creation_5m_per_million: money(entry, "cache_creation_5m"),
            cache_creation_30m_per_million: money(entry, "cache_creation_30m"),
            cache_creation_1h_per_million: money(entry, "cache_creation_1h"),
            reasoning_per_million: None,
            image_input_per_million: None,
            image_output_per_million: money(entry, "image_output"),
            audio_input_per_million: None,
            cached_audio_input_per_million: None,
            audio_output_per_million: None,
            video_input_per_million: None,
            video_per_million: None,
        })
        .collect()
}

fn price_rate(row: &document::PriceRate) -> PriceRateDto {
    PriceRateDto {
        id: ids::id("price_rates", row.id),
        price_rule_id: ids::id("price_rules", row.rule_id),
        // The metric keys are v3's, kept verbatim by v4 — see
        // `gproxy_store::entity::pricing::metric`.
        metric: row.metric.clone(),
        unit: price_unit(&row.metric).to_owned(),
        unit_quantity: row.unit_size.max(1).to_string(),
        value: row.price.clone(),
        conditions: row.conditions.clone(),
        priority: i32::try_from(row.priority).unwrap_or(i32::MAX),
    }
}

/// v3 had no unit column: the metric name said what it counted, and v4 made
/// that explicit. Reading it back off the name is exact for every metric v4
/// defines and lands on `count` for a custom one, which is what a custom metric
/// has always meant.
fn price_unit(metric: &str) -> &'static str {
    if metric.ends_with("_tokens") {
        "token"
    } else if metric.ends_with("_seconds") {
        "second"
    } else if metric.ends_with("_characters") {
        "character"
    } else {
        "count"
    }
}

// --------------------------------------------------------------- budgets --

/// v3's six budget columns as v4 rows. `subject_kind` becomes `owner_kind`
/// under the names the admission chain uses (`api_key`, `user`, `team`, `org`,
/// `credential`), and each non-null column becomes one row keyed by the column
/// it came from.
fn quotas(data: &document::Data, report: &mut Report) -> Vec<QuotaDto> {
    let mut out = Vec::new();
    for row in &data.quotas {
        let Some((owner_kind, owner_id)) = owner(&row.subject_kind, row.subject_id) else {
            report.drop_row(
                "quotas",
                format!("quota {} ({}:{})", row.id, row.subject_kind, row.subject_id),
                "v4 has no budget owner of that kind",
            );
            continue;
        };
        for (window, period, limit) in [
            ("total", "total", &row.quota_total),
            ("daily", "1d", &row.quota_daily),
            ("weekly", "7d", &row.quota_weekly),
            ("monthly", "1m", &row.quota_monthly),
            ("5h", "5h", &row.quota_5h),
            ("7d", "7d", &row.quota_7d),
        ] {
            let Some(limit) = limit.as_deref().map(str::trim).filter(|v| !v.is_empty()) else {
                continue;
            };
            if window == "weekly" {
                report.warn(format!(
                    "quota {} had a weekly budget: v4 has no calendar week, so it became a \
                     fixed seven-day window whose first period opens at the first request",
                    row.id
                ));
            }
            out.push(QuotaDto {
                id: ids::part("quotas", row.id, window),
                owner_kind: owner_kind.to_owned(),
                owner_id: owner_id.clone(),
                window_key: window.to_owned(),
                // v3's budgets were money, always.
                metric: "cost".into(),
                unit: CURRENCY.into(),
                limit_value: limit.to_owned(),
                period: period.to_owned(),
                period_seconds: None,
                anchor_at_ms: None,
                model_pattern: None,
                enabled: row.enabled,
            });
        }
    }
    out
}

/// v3's per-credential request ceiling, as the operator limit v4 spells it
/// with. `tpm_limit` has no v4 form and is reported by [`credentials`].
fn credential_limits(data: &document::Data, report: &mut Report) -> Vec<QuotaDto> {
    let mut out = Vec::new();
    for row in &data.credentials {
        let Some(rpm) = row.config.rpm_limit.filter(|limit| *limit > 0) else {
            continue;
        };
        out.push(QuotaDto {
            id: ids::part("credentials", row.config.id, "rpm"),
            owner_kind: "credential".into(),
            owner_id: ids::id("credentials", row.config.id),
            window_key: "rpm".into(),
            metric: "requests".into(),
            unit: "count".into(),
            limit_value: rpm.to_string(),
            period: format!("{RPM_PERIOD_SECONDS}s"),
            period_seconds: Some(RPM_PERIOD_SECONDS),
            anchor_at_ms: None,
            model_pattern: None,
            enabled: true,
        });
        report.warn(format!(
            "credential {} had rpm_limit {rpm}: it became a `requests` limit over \
             {RPM_PERIOD_SECONDS} seconds, which v4 meters per instance rather than per process",
            row.config.id
        ));
    }
    out
}

/// v3's four permission and budget subject kinds as v4's owner kinds. `user_key`
/// is v4's `api_key`; the rest keep their meaning and lose their spelling.
pub fn owner(subject_kind: &str, subject_id: i64) -> Option<(&'static str, String)> {
    Some(match subject_kind {
        "user" => ("user", ids::id("users", subject_id)),
        "user_key" => ("api_key", ids::id("user_keys", subject_id)),
        "team" => ("team", ids::id("teams", subject_id)),
        "organization" => ("org", ids::id("organizations", subject_id)),
        "credential" => ("credential", ids::id("credentials", subject_id)),
        _ => return None,
    })
}

// --------------------------------------------------------------- rewrite --

fn rewrite_rules(
    data: &document::Data,
    report: &mut Report,
) -> (Vec<RewriteRuleDto>, Vec<RuleSetDto>) {
    let sets: Vec<RuleSetDto> = data
        .rule_sets
        .iter()
        .map(|row| RuleSetDto {
            id: ids::id("rule_sets", row.id),
            name: row.name.clone(),
            description: row.description.clone(),
            enabled: row.enabled,
            created_at_ms: 0,
            updated_at_ms: 0,
        })
        .collect();

    let mut rules = Vec::new();
    for row in &data.rules {
        let named = format!("rule {} ({})", row.id, row.config.kind());
        let RuleConfig::Transform {
            phase,
            locate,
            actions,
            ..
        } = &row.config
        else {
            report.drop_row("rules", named, unmappable_rule(&row.config));
            continue;
        };
        let paths = match locate {
            Locate::Path(path) => vec![path.clone()],
            Locate::Paths(paths) => paths.clone(),
            Locate::Match(_) => {
                report.drop_row(
                    "rules",
                    named,
                    "it located its text by a whole-body match; v4 addresses the body by path",
                );
                continue;
            }
        };
        // v4 filters by `(operation, dialect)` pairs and has no wildcard for
        // the dialect half. Widening the rule to every dialect would change
        // what it does, so the rule is refused rather than broadened.
        if let Some(operations) = row.filter_operations.as_ref().filter(|ops| !ops.is_empty()) {
            report.drop_row(
                "rules",
                named,
                format!(
                    "it was filtered to operations {}; v4 filters on operation *and* dialect \
                     pairs, and guessing the dialect would change what the rule matches",
                    operations.join(", ")
                ),
            );
            continue;
        }
        for (index, action) in actions.iter().enumerate() {
            let (pattern, replacement) = match action {
                Action::ReplaceRegex { pattern, with } => (pattern.clone(), with.clone()),
                Action::ReplaceText {
                    from: Some(from),
                    with,
                } => (regex::escape(from), with.clone()),
                Action::ReplaceText { from: None, .. } => {
                    report.drop_row(
                        "rules",
                        format!("{} action {index}", named),
                        "it replaced the whole located value, which v4's pattern-and-replacement \
                         rewrite has no form for",
                    );
                    continue;
                }
            };
            rules.push(RewriteRuleDto {
                id: ids::part("rules", row.id, &index.to_string()),
                rule_set_id: ids::id("rule_sets", row.rule_set_id),
                phase: match phase.as_str() {
                    "response" => "response",
                    "both" => "both",
                    _ => "request",
                }
                .to_owned(),
                target: "body".into(),
                target_name: None,
                paths: Some(Value::Array(
                    paths.iter().map(|path| Value::from(path.as_str())).collect(),
                )),
                pattern,
                replacement,
                filter_operation_keys: None,
                filter_model_pattern: row.filter_model_pattern.clone(),
                filter_header_pattern: row.filter_header_pattern.clone(),
                filter_event_pattern: None,
                sort_order: row.sort_order,
                enabled: row.enabled,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
        }
    }
    (rules, sets)
}

fn unmappable_rule(config: &RuleConfig) -> &'static str {
    match config {
        RuleConfig::SystemText { .. } => {
            "it prepended or appended system text; v4's rewrite rules replace what is there and \
             cannot add a turn"
        }
        RuleConfig::CacheBreakpoint { .. } => {
            "it inserted a cache breakpoint; v4 has no rule that constructs one"
        }
        RuleConfig::Rewrite { .. } => {
            "it set, deleted or merged a JSON value; v4's rewrite rules are text replacements \
             over a path, not structural edits"
        }
        RuleConfig::Header { .. } => {
            "it set a header; v4's header rules replace inside a header that is already there"
        }
        RuleConfig::Transform { .. } => unreachable!("transform rules are translated"),
    }
}

fn provider_rule_set(row: &document::ProviderRuleSet, timestamp: i64) -> ProviderRuleSetDto {
    ProviderRuleSetDto {
        id: ids::id("provider_rule_sets", row.id),
        provider_id: ids::id("providers", row.provider_id),
        rule_set_id: ids::id("rule_sets", row.rule_set_id),
        sort_order: row.sort_order,
        enabled: row.enabled,
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
    }
}

// ------------------------------------------------------------ the rest --

/// The v3 tables with no v4 form at all, named row by row. See the module note
/// for why each one is here rather than translated.
fn refuse_unmappable(data: &document::Data, report: &mut Report) {
    for row in &data.aliases {
        report.drop_row(
            "aliases",
            format!("alias {} ({} -> {})", row.id, row.alias, row.target),
            "v4 has no alias table: a public name is an exposed model on a route, and which \
             providers should serve it is not recorded in a v3 alias",
        );
    }
    // Reported once rather than once per row: a provider has one of these per
    // operation and dialect, and a hundred identical lines would bury the rest.
    if !data.routing_rules.is_empty() {
        report.drop_row(
            "routing_rules",
            format!("{} rows", data.routing_rules.len()),
            "v3 seeded these from its channels' own declarations; v4 asks the channel at \
             assembly and only stores a row to override it, so importing them would pin every \
             provider to what v3 supported",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn document(data: Value) -> Document {
        serde_json::from_value(json!({"format_version": 1, "data": data})).unwrap()
    }

    fn translated(data: Value) -> Configuration {
        translate(&document(data), &Bridge::new(None).unwrap(), 1_700_000_000_000).unwrap()
    }

    #[test]
    fn a_provider_keeps_its_label_its_channel_and_its_base_url() {
        let out = translated(json!({"providers": [{
            "id": 3, "name": "anthropic", "label": "Anthropic (prod)",
            "channel": "claudeapi", "enabled": true,
            "settings": {"base_url": "https://api.anthropic.com", "beta": true}
        }]}));
        let provider = &out.export.data.providers[0];
        assert_eq!(provider.id, "v3-providers-3");
        assert_eq!(provider.name, "Anthropic (prod)");
        assert_eq!(provider.channel, "claudeapi");
        assert_eq!(provider.base_url.as_deref(), Some("https://api.anthropic.com"));
        // And the key stays in `config`, because a channel may still read it.
        assert_eq!(provider.config["beta"], json!(true));
        assert_eq!(provider.config["base_url"], json!("https://api.anthropic.com"));
    }

    #[test]
    fn a_provider_without_a_label_keeps_its_name() {
        let out = translated(json!({"providers": [
            {"id": 1, "name": "openai", "channel": "openai", "settings": null, "label": "  "}
        ]}));
        assert_eq!(out.export.data.providers[0].name, "openai");
        // A null `settings_json` becomes the empty object v4 requires.
        assert_eq!(out.export.data.providers[0].config, json!({}));
    }

    #[test]
    fn one_profile_is_minted_per_distinct_proxy_and_shared_by_everything_that_used_it() {
        let out = translated(json!({
            "providers": [
                {"id": 1, "name": "a", "channel": "openai", "proxy_url": "http://p:8080"},
                {"id": 2, "name": "b", "channel": "openai", "proxy_url": "http://p:8080"},
                {"id": 3, "name": "c", "channel": "openai"}
            ]
        }));
        assert_eq!(out.export.data.connection_profiles.len(), 1);
        let profile = &out.export.data.connection_profiles[0];
        assert_eq!(profile.proxy_mode, "explicit");
        assert_eq!(profile.proxy_url.as_deref(), Some("http://p:8080"));
        let ids: Vec<_> = out
            .export
            .data
            .providers
            .iter()
            .map(|row| row.connection_profile_id.clone())
            .collect();
        assert_eq!(
            ids,
            [Some(profile.id.clone()), Some(profile.id.clone()), None]
        );
    }

    #[test]
    fn a_credential_without_a_secret_stops_the_import_rather_than_arriving_broken() {
        let document = document(json!({"credentials": [
            {"config": {"id": 1, "provider_id": 1, "label": "prod"}, "secret": null}
        ]}));
        let error = translate(&document, &Bridge::new(None).unwrap(), 0)
            .unwrap_err()
            .to_string();
        assert!(error.contains("include_secrets"), "{error}");
    }

    #[test]
    fn a_credentials_secret_is_re_sealed_for_the_sdk_and_its_limits_become_quotas() {
        let out = translated(json!({"credentials": [{
            "config": {"id": 5, "provider_id": 3, "label": "key one", "kind": "oauth",
                       "version": 9, "enabled": true, "weight": 250,
                       "rpm_limit": 60, "tpm_limit": 100000},
            "secret": {"ciphertext": [123, 34, 97, 34, 58, 49, 125]}
        }]}));
        let credential = &out.export.data.credentials[0];
        assert_eq!(credential.credential.id, "v3-credentials-5");
        assert_eq!(credential.credential.provider_id, "v3-providers-3");
        assert_eq!(credential.credential.auth_kind, "oauth");
        assert_eq!(credential.credential.version, 9);
        assert_eq!(credential.credential.status, "active");
        assert!(credential.secret.is_some());

        let quota = &out.export.data.quotas[0];
        assert_eq!(quota.owner_kind, "credential");
        assert_eq!(quota.owner_id, "v3-credentials-5");
        assert_eq!(quota.metric, "requests");
        assert_eq!(quota.limit_value, "60");
        assert_eq!(quota.period_seconds, Some(60));

        // The weight and the token ceiling have no v4 form and are named.
        assert!(out.report.warnings.iter().any(|w| w.contains("weight 250")));
        assert!(out.report.dropped.iter().any(|d| d.table == "credentials.tpm_limit"));
    }

    #[test]
    fn one_v3_quota_row_becomes_one_v4_row_per_period_it_set() {
        let out = translated(json!({"quotas": [{
            "id": 2, "subject_kind": "user_key", "subject_id": 7,
            "quota_total": "100", "quota_daily": "10", "quota_weekly": "50",
            "enabled": true
        }]}));
        let quotas = &out.export.data.quotas;
        assert_eq!(quotas.len(), 3);
        assert_eq!(quotas[0].id, "v3-quotas-2-total");
        assert_eq!(quotas[0].period, "total");
        assert_eq!(quotas[1].period, "1d");
        assert_eq!(quotas[2].period, "7d");
        // The subject became v4's owner chain spelling, pointing at the key.
        assert_eq!(quotas[0].owner_kind, "api_key");
        assert_eq!(quotas[0].owner_id, "v3-user_keys-7");
        assert_eq!(quotas[0].metric, "cost");
        // And the one period that changed meaning says so.
        assert!(out.report.warnings.iter().any(|w| w.contains("weekly")));
    }

    #[test]
    fn a_quota_on_a_subject_v4_has_no_owner_for_is_reported_not_written() {
        let out = translated(json!({"quotas": [
            {"id": 1, "subject_kind": "surface", "subject_id": 1, "quota_total": "5"}
        ]}));
        assert!(out.export.data.quotas.is_empty());
        assert_eq!(out.report.dropped[0].table, "quotas");
    }

    #[test]
    fn prices_keep_their_metric_keys_and_their_tiers_become_rows() {
        let out = translated(json!({
            "price_rules": [{"id": 1, "provider_id": 2, "model_pattern": "claude-*",
                             "priority": 10, "enabled": true,
                             "tiers": [{"service_tier": "batch", "multiplier": "0.5"},
                                       {"min_prompt_tokens": 200000, "input": "6"}]}],
            "price_rates": [{"id": 4, "rule_id": 1, "metric": "input_tokens",
                             "unit_size": 1000000, "price": "3.00", "priority": 0},
                            {"id": 5, "rule_id": 1, "metric": "web_searches",
                             "unit_size": 1, "price": "0.01", "priority": 0}]
        }));
        let rule = &out.export.data.price_rules[0];
        assert_eq!(rule.id, "v3-price_rules-1");
        assert_eq!(rule.provider_id.as_deref(), Some("v3-providers-2"));
        assert_eq!(rule.currency, "USD");

        let rates = &out.export.data.price_rates;
        assert_eq!(rates[0].metric, "input_tokens");
        assert_eq!(rates[0].unit, "token");
        assert_eq!(rates[0].unit_quantity, "1000000");
        assert_eq!(rates[1].unit, "count");

        let tiers = &out.export.data.price_tiers;
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].service_tier.as_deref(), Some("batch"));
        assert_eq!(tiers[0].multiplier.as_deref(), Some("0.5"));
        assert_eq!(tiers[1].min_prompt_tokens, 200000);
        assert_eq!(tiers[1].input_per_million.as_deref(), Some("6"));
    }

    #[test]
    fn a_transform_rule_becomes_one_rewrite_per_action() {
        let out = translated(json!({
            "rule_sets": [{"id": 1, "name": "set", "enabled": true}],
            "rules": [{"id": 9, "rule_set_id": 1, "sort_order": 3, "enabled": true,
                       "filter_model_pattern": "gpt-*",
                       "config": {"kind": "transform", "phase": "response",
                                  "locate": {"type": "paths", "value": ["a.b", "c.*"]},
                                  "actions": [
                                    {"op": "replace_regex", "pattern": "x+", "with": "y"},
                                    {"op": "replace_text", "from": "a.b(", "with": "z"}]}}]
        }));
        let rules = &out.export.data.rewrite_rules;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "v3-rules-9-0");
        assert_eq!(rules[0].rule_set_id, "v3-rule_sets-1");
        assert_eq!(rules[0].phase, "response");
        assert_eq!(rules[0].target, "body");
        assert_eq!(rules[0].paths, Some(json!(["a.b", "c.*"])));
        assert_eq!(rules[0].pattern, "x+");
        assert_eq!(rules[0].filter_model_pattern.as_deref(), Some("gpt-*"));
        // Literal text becomes a pattern that matches exactly that text.
        assert_eq!(rules[1].pattern, regex::escape("a.b("));
        assert_eq!(rules[1].replacement, "z");
    }

    #[test]
    fn the_four_rule_kinds_v4_cannot_perform_are_named_one_by_one() {
        let out = translated(json!({"rules": [
            {"id": 1, "rule_set_id": 1, "config": {"kind": "system_text", "text": "hi",
                                                   "position": "append"}},
            {"id": 2, "rule_set_id": 1, "config": {"kind": "cache_breakpoint",
                                                   "target": "system"}},
            {"id": 3, "rule_set_id": 1, "config": {"kind": "rewrite", "path": "a",
                                                   "action": "set", "value": 1}},
            {"id": 4, "rule_set_id": 1, "config": {"kind": "header", "name": "x",
                                                   "value": "y", "mode": "override"}}
        ]}));
        assert!(out.export.data.rewrite_rules.is_empty());
        assert_eq!(out.report.dropped.len(), 4);
        assert!(out.report.dropped.iter().all(|d| d.table == "rules"));
        assert!(out.report.dropped[0].reason.contains("cannot add a turn"));
    }

    #[test]
    fn a_transform_filtered_to_operations_is_refused_rather_than_widened() {
        let out = translated(json!({"rules": [{
            "id": 7, "rule_set_id": 1, "filter_operations": ["generate_content"],
            "config": {"kind": "transform", "phase": "request",
                       "locate": {"type": "path", "value": "a"},
                       "actions": [{"op": "replace_regex", "pattern": "x", "with": "y"}]}}]}));
        assert!(out.export.data.rewrite_rules.is_empty());
        assert!(out.report.dropped[0].reason.contains("dialect"));
    }

    #[test]
    fn aliases_and_routing_rules_are_reported_as_work_to_redo() {
        let out = translated(json!({
            "aliases": [{"id": 1, "alias": "gpt4", "target": "gpt-4-turbo", "enabled": true}],
            "routing_rules": [
                {"id": 1, "provider_id": 1, "operation": "generate_content",
                 "kind": "openai", "implementation": "passthrough"},
                {"id": 2, "provider_id": 1, "operation": "generate_content",
                 "kind": "anthropic", "implementation": "transform_to"}
            ]
        }));
        assert!(out.export.data.operation_rules.is_empty());
        let tables: Vec<_> = out.report.dropped.iter().map(|d| d.table).collect();
        assert_eq!(tables, ["aliases", "routing_rules"]);
        // The routing rules are one line, not one line per row.
        assert!(out.report.dropped[1].row.contains("2 rows"));
    }

    #[test]
    fn routes_members_and_exposed_names_carry_their_ids_across() {
        let out = translated(json!({
            "routes": [{"id": 1, "name": "claude", "strategy": "failover",
                        "max_attempts": 0, "enabled": true}],
            "route_members": [{"id": 2, "route_id": 1, "provider_id": 3,
                               "upstream_model": "claude-sonnet-4", "tier": 0,
                               "weight": 0, "enabled": true}],
            "model_aliases": [{"id": 4, "name": "sonnet", "route_id": 1, "enabled": true}]
        }));
        assert_eq!(out.export.data.routes[0].strategy, "failover");
        // v4 refuses a zero, so a v3 row that predates the column is clamped.
        assert_eq!(out.export.data.routes[0].max_attempts, 1);
        assert_eq!(out.export.data.route_members[0].weight, 1);
        assert_eq!(out.export.data.route_members[0].route_id, "v3-routes-1");
        assert_eq!(out.export.data.exposed_models[0].id, "v3-model_aliases-4");
        assert_eq!(out.export.data.exposed_models[0].route_id, "v3-routes-1");
    }

    #[test]
    fn the_document_it_produces_is_the_version_the_sdk_reads() {
        let out = translated(json!({}));
        assert_eq!(out.export.format_version, EXPORT_FORMAT_VERSION);
        assert!(!out.export.secrets_omitted);
        assert!(out.export.data.settings.is_none());
    }
}
