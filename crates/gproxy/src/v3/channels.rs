//! v3's channel ids as v4's, which is not a table of renames.
//!
//! # Why this is not a rename table
//!
//! v3 had 27 channels and v4 has 25, and the overlap is not the whole of
//! either. Reading production's twelve providers turned up three distinct
//! problems in ten channel ids:
//!
//! 1. **`aws-bedrock` is `aws_bedrock`** — a hyphen where v4 writes an
//!    underscore. Copying the column verbatim produces a provider naming a
//!    channel that is not registered, which v4 treats as a configuration error
//!    rather than falling back to anything.
//! 2. **`cloudflare-ai-gateway` has no v4 channel at all.** v4's decision was
//!    that a vendor whose whole difference is an origin and a header is a
//!    `custom` provider, not a channel — `crates/gproxy-channel/README.md`,
//!    "Vendors That Need No Channel". So the row has to be *rebuilt*: an origin
//!    assembled from a field that lived in the **credential secret**, a static
//!    header, and a dialect list. The same goes for `nvidia` and `vercel`.
//! 3. Everything else keeps its id, and is checked against the registry rather
//!    than assumed.
//!
//! A channel id that reaches none of those **fails the migration**, naming the
//! provider. The alternative is a row that imports and then cannot serve a
//! request, which is the failure mode this whole migration exists to avoid.
//!
//! # The three `custom` recipes
//!
//! Each is v4's README recipe, filled in from what v3 actually stored. The
//! interesting one is Cloudflare, because v3 kept `account_id` and `gateway_id`
//! in the *credential*, and v4 needs the account in the provider's `base_url`
//! (`v3:crates/gproxy-channels/src/cloudflare_ai_gateway/prepare.rs`). A
//! provider is therefore only translatable when its credentials agree on one
//! account — which is the normal case, and a loud failure when it is not.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{Error, Result};

/// Every channel id v4 registers, as `gproxy_channel::channels::compiled_in()`
/// declares them. Listed rather than queried because a build with a reduced
/// feature set must still translate a full document: a provider whose channel
/// this binary was not compiled with is a runtime concern the operator can fix
/// by rebuilding, not a reason to refuse their configuration.
pub const V4_CHANNELS: [&str; 25] = [
    "aistudio",
    "antigravity",
    "aws_bedrock",
    "azure",
    "claudeapi",
    "claudecode",
    "claudeweb",
    "cline",
    "codex",
    "copilotcli",
    "custom",
    "dashscope",
    "deepseek",
    "devin",
    "geminicli",
    "grokbuild",
    "kimi",
    "kiro",
    "openai",
    "opencode",
    "openrouter",
    "vertex",
    "vertexexpress",
    "workbuddy",
    "xai",
];

/// What to do with one v3 channel id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// v4 registers the same id.
    Keep,
    /// v4 spells it differently and nothing else changes.
    Rename(&'static str),
    /// v4 has no channel for this vendor and does not want one: the provider
    /// becomes `custom`. See [`Recipe`].
    Custom(Recipe),
}

/// A vendor v4 serves through `custom`, and what has to be assembled for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Recipe {
    /// `base_url` is the origin v3 used, and the dialect list is all it needs.
    Origin {
        default_base_url: &'static str,
        dialects: &'static [&'static str],
    },
    /// Cloudflare: the account id moves out of the credential and into the
    /// path, and the gateway id becomes a static header.
    CloudflareGateway,
}

/// v3's channel ids, every one of them, checked against v3's own descriptors
/// (`git grep 'id: "' v3 -- 'crates/gproxy-channels/src/*/mod.rs'`).
const RULES: [(&str, Rule); 27] = [
    ("aistudio", Rule::Keep),
    ("antigravity", Rule::Keep),
    // The rename production tripped over.
    ("aws-bedrock", Rule::Rename("aws_bedrock")),
    ("azure", Rule::Keep),
    ("claudeapi", Rule::Keep),
    ("claudecode", Rule::Keep),
    ("claudeweb", Rule::Keep),
    ("cline", Rule::Keep),
    (
        "cloudflare-ai-gateway",
        Rule::Custom(Recipe::CloudflareGateway),
    ),
    ("codex", Rule::Keep),
    ("copilotcli", Rule::Keep),
    ("custom", Rule::Keep),
    ("dashscope", Rule::Keep),
    ("deepseek", Rule::Keep),
    ("geminicli", Rule::Keep),
    ("grokbuild", Rule::Keep),
    ("kimi", Rule::Keep),
    ("kiro", Rule::Keep),
    (
        "nvidia",
        Rule::Custom(Recipe::Origin {
            default_base_url: "https://integrate.api.nvidia.com",
            dialects: &["openai_chat"],
        }),
    ),
    ("openai", Rule::Keep),
    ("opencode", Rule::Keep),
    ("openrouter", Rule::Keep),
    (
        "vercel",
        Rule::Custom(Recipe::Origin {
            default_base_url: "https://ai-gateway.vercel.sh",
            dialects: &["openai_chat", "openai", "claude"],
        }),
    ),
    ("vertex", Rule::Keep),
    ("vertexexpress", Rule::Keep),
    ("workbuddy", Rule::Keep),
    ("xai", Rule::Keep),
];

/// v3's two pairs of legacy aliases, which its own `lib.rs` folded before
/// looking a channel up. A database written by an older v3 can still hold one.
const ALIASES: [(&str, &str); 4] = [
    ("kimiapi", "kimi"),
    ("kimicode", "kimi"),
    ("opencodezen", "opencode"),
    ("opencodego", "opencode"),
];

/// v3's default origin for a vendor that stored none.
const CLOUDFLARE_DEFAULT_BASE_URL: &str = "https://api.cloudflare.com";
/// v3's default when a credential named no gateway.
const CLOUDFLARE_DEFAULT_GATEWAY: &str = "default";

/// One v3 provider as a v4 one: the channel it speaks, the origin, the config,
/// and what to drop from its credentials because it moved into the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub channel: String,
    pub base_url: Option<String>,
    pub config: Value,
    /// Keys to remove from each of this provider's credential secrets. Nonempty
    /// only for a recipe that lifted something out of them, and the reason it
    /// matters is that leaving a stale `account_id` in a `custom` credential
    /// would be a second, disagreeing copy of the origin.
    pub strip_from_secrets: Vec<&'static str>,
    /// What changed, for the report. None when the id carried straight across.
    pub note: Option<String>,
}

/// Translate one provider. `secrets` are its credentials' opened secrets,
/// which two of the recipes need and the rest ignore.
///
/// `name` is only for the error: an operator with twelve providers needs to be
/// told which one stopped the migration.
pub fn provider(
    v3_channel: &str,
    name: &str,
    settings: &Value,
    secrets: &[Value],
) -> Result<Provider> {
    let id = v3_channel.trim();
    let canonical = ALIASES
        .iter()
        .find(|(alias, _)| *alias == id)
        .map(|(_, target)| *target)
        .unwrap_or(id);

    let Some((_, rule)) = RULES.iter().find(|(known, _)| *known == canonical) else {
        return Err(unknown_channel(id, name));
    };
    match rule {
        Rule::Keep => {
            // Checked rather than trusted: `RULES` and the registry are two
            // lists, and a channel dropped from v4 without this table being
            // updated would otherwise import as a dead row.
            if !V4_CHANNELS.contains(&canonical) {
                return Err(retired_channel(canonical, name));
            }
            Ok(Provider {
                channel: canonical.to_owned(),
                base_url: base_url(settings),
                config: object(settings),
                strip_from_secrets: Vec::new(),
                note: (canonical != id)
                    .then(|| format!("channel `{id}` is v3's old name for `{canonical}`")),
            })
        }
        Rule::Rename(v4) => Ok(Provider {
            channel: (*v4).to_owned(),
            base_url: base_url(settings),
            config: object(settings),
            strip_from_secrets: Vec::new(),
            note: Some(format!("channel `{id}` is `{v4}` in v4")),
        }),
        Rule::Custom(recipe) => custom(*recipe, id, name, settings, secrets),
    }
}

/// A vendor v4 serves through `custom`.
fn custom(
    recipe: Recipe,
    id: &str,
    name: &str,
    settings: &Value,
    secrets: &[Value],
) -> Result<Provider> {
    match recipe {
        Recipe::Origin {
            default_base_url,
            dialects,
        } => Ok(Provider {
            channel: "custom".to_owned(),
            base_url: Some(base_url(settings).unwrap_or_else(|| default_base_url.to_owned())),
            config: merged(settings, json!({"dialects": dialects})),
            strip_from_secrets: Vec::new(),
            note: Some(format!(
                "channel `{id}` has no v4 channel: v4 serves this vendor as a `custom` \
                 provider, and the row was rebuilt as one"
            )),
        }),
        Recipe::CloudflareGateway => {
            // v3 kept the account in the *credential*; v4 needs it in the
            // provider's origin, so every credential of this provider has to
            // agree on one.
            let accounts: Vec<&str> = distinct(secrets, "account_id");
            let account = match accounts.as_slice() {
                [account] => *account,
                [] => {
                    return Err(Error::other(format!(
                        "provider `{name}` uses v3's `cloudflare-ai-gateway`, which v4 serves as \
                         a `custom` provider whose `base_url` contains the Cloudflare account \
                         id. None of its credentials carries an `account_id`, so the origin \
                         cannot be assembled. Create the provider by hand in v4 with \
                         base_url = https://api.cloudflare.com/client/v4/accounts/<account>/ai \
                         and remove it from the export."
                    )));
                }
                many => {
                    return Err(Error::other(format!(
                        "provider `{name}` uses v3's `cloudflare-ai-gateway` and its credentials \
                         name {} different Cloudflare accounts ({}). v4's `base_url` is one \
                         origin per provider, so this is one v4 provider per account: split it \
                         by hand before exporting.",
                        many.len(),
                        many.join(", ")
                    )));
                }
            };
            let origin = base_url(settings)
                .unwrap_or_else(|| CLOUDFLARE_DEFAULT_BASE_URL.to_owned())
                .trim_end_matches('/')
                .to_owned();
            let gateway = distinct(secrets, "gateway_id")
                .first()
                .copied()
                .unwrap_or(CLOUDFLARE_DEFAULT_GATEWAY);
            Ok(Provider {
                channel: "custom".to_owned(),
                base_url: Some(format!("{origin}/client/v4/accounts/{account}/ai")),
                config: merged(
                    settings,
                    json!({
                        "dialects": ["openai_chat", "openai", "claude"],
                        "headers": {"cf-aig-gateway-id": gateway},
                    }),
                ),
                // Both moved into the provider row; a copy left behind in the
                // credential would be a second, disagreeing source of truth.
                strip_from_secrets: vec!["account_id", "gateway_id"],
                note: Some(format!(
                    "channel `{id}` has no v4 channel: the row was rebuilt as a `custom` \
                     provider with the Cloudflare account `{account}` in its base URL and \
                     gateway `{gateway}` as a static header"
                )),
            })
        }
    }
}

/// The distinct non-blank string values of `field` across some secrets, in
/// first-seen order.
fn distinct<'a>(secrets: &'a [Value], field: &str) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for secret in secrets {
        let Some(value) = secret
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if !out.contains(&value) {
            out.push(value);
        }
    }
    out
}

/// v3 kept the upstream origin inside `settings_json`; v4 has a column.
fn base_url(settings: &Value) -> Option<String> {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
}

/// v4 requires `config` to be an object; v3 stored whatever the channel put
/// there, and an older row can hold `null`.
fn object(settings: &Value) -> Value {
    match settings {
        Value::Object(_) => settings.clone(),
        _ => Value::Object(serde_json::Map::new()),
    }
}

/// v3's settings with a recipe's keys written over them. The recipe wins: it is
/// what makes the provider work at all, and a v3 `dialects` key meant something
/// to a channel that no longer exists.
fn merged(settings: &Value, recipe: Value) -> Value {
    let mut map: BTreeMap<String, Value> = match object(settings) {
        Value::Object(existing) => existing.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    if let Value::Object(fields) = recipe {
        map.extend(fields);
    }
    Value::Object(map.into_iter().collect())
}

fn unknown_channel(id: &str, name: &str) -> Error {
    Error::other(format!(
        "provider `{name}` uses channel `{id}`, which this migration has no rule for. It is \
         neither one of v3's 27 channels nor one of v4's {}. The import is refused rather than \
         writing a provider that cannot serve a request: create this provider by hand in v4 and \
         remove it from the export.",
        V4_CHANNELS.len()
    ))
}

fn retired_channel(id: &str, name: &str) -> Error {
    Error::other(format!(
        "provider `{name}` uses channel `{id}`, which v3 had and v4 does not register. This \
         migration's channel table says to keep the id, and the registry disagrees — one of the \
         two is out of date. Refused rather than imported as a dead row."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn translate(channel: &str, settings: Value, secrets: &[Value]) -> Provider {
        provider(channel, "p", &settings, secrets).expect("translates")
    }

    /// Every v3 channel id is either kept, renamed or rebuilt — none is
    /// unhandled. This is the test that fails when a channel is added to
    /// either side without this table being updated.
    #[test]
    fn every_v3_channel_has_a_rule_and_every_kept_one_is_registered_in_v4() {
        assert_eq!(RULES.len(), 27, "v3 had 27 channel descriptors");
        for (id, rule) in RULES {
            match rule {
                Rule::Keep => assert!(
                    V4_CHANNELS.contains(&id),
                    "`{id}` is kept but v4 does not register it"
                ),
                Rule::Rename(v4) => assert!(
                    V4_CHANNELS.contains(&v4),
                    "`{id}` renames to `{v4}`, which v4 does not register"
                ),
                Rule::Custom(_) => assert!(
                    !V4_CHANNELS.contains(&id),
                    "`{id}` is rebuilt as custom, but v4 registers it under its own name"
                ),
            }
        }
        // And the table is sorted, so a reader can find an id in it.
        let mut ids: Vec<&str> = RULES.iter().map(|(id, _)| *id).collect();
        let given = ids.clone();
        ids.sort();
        assert_eq!(ids, given, "the rule table is not in id order");
    }

    /// The registry list is the one thing here that is a copy of something
    /// else, so it is checked against the real thing. Only one way round: a
    /// build with a reduced feature set compiles in fewer channels, and that
    /// must not fail this test.
    #[test]
    fn the_registry_list_covers_what_v4_compiles_in() {
        for channel in gproxy_sdk::channels::compiled_in() {
            let id = channel.descriptor().id;
            assert!(
                V4_CHANNELS.contains(&id),
                "`{id}` is compiled in but missing from V4_CHANNELS"
            );
        }
    }

    #[test]
    fn a_channel_v4_spells_the_same_way_carries_straight_across() {
        let out = translate("claudecode", json!({"beta": true}), &[]);
        assert_eq!(out.channel, "claudecode");
        assert_eq!(out.config, json!({"beta": true}));
        assert!(out.note.is_none());
        assert!(out.strip_from_secrets.is_empty());
    }

    /// The one production tripped over.
    #[test]
    fn aws_bedrock_loses_its_hyphen() {
        let out = translate("aws-bedrock", json!({"region": "us-east-1"}), &[]);
        assert_eq!(out.channel, "aws_bedrock");
        assert_eq!(out.config, json!({"region": "us-east-1"}));
        assert!(out.note.unwrap().contains("`aws_bedrock` in v4"));
    }

    #[test]
    fn v3s_own_legacy_aliases_still_resolve() {
        for (alias, target) in ALIASES {
            let out = translate(alias, json!({}), &[]);
            assert_eq!(out.channel, target);
            assert!(out.note.unwrap().contains("old name"));
        }
    }

    #[test]
    fn a_base_url_inside_v3s_settings_becomes_v4s_column() {
        let out = translate("custom", json!({"base_url": " https://x.invalid "}), &[]);
        assert_eq!(out.base_url.as_deref(), Some("https://x.invalid"));
        // And it stays in `config` too, because a channel may still read it.
        assert_eq!(out.config["base_url"], json!(" https://x.invalid "));
    }

    #[test]
    fn a_null_settings_column_becomes_the_object_v4_requires() {
        assert_eq!(translate("openai", Value::Null, &[]).config, json!({}));
    }

    #[test]
    fn nvidia_and_vercel_become_custom_with_v4s_own_recipe() {
        let nvidia = translate("nvidia", json!({}), &[]);
        assert_eq!(nvidia.channel, "custom");
        assert_eq!(
            nvidia.base_url.as_deref(),
            Some("https://integrate.api.nvidia.com")
        );
        assert_eq!(nvidia.config["dialects"], json!(["openai_chat"]));

        // A v3 row that overrode the origin keeps its own.
        let vercel = translate("vercel", json!({"base_url": "https://gw.invalid"}), &[]);
        assert_eq!(vercel.channel, "custom");
        assert_eq!(vercel.base_url.as_deref(), Some("https://gw.invalid"));
        assert_eq!(
            vercel.config["dialects"],
            json!(["openai_chat", "openai", "claude"])
        );
        assert!(vercel.note.unwrap().contains("no v4 channel"));
    }

    /// The interesting one: v3 kept the account in the credential and v4 needs
    /// it in the origin.
    #[test]
    fn cloudflare_moves_the_account_out_of_the_credential_and_into_the_origin() {
        let out = translate(
            "cloudflare-ai-gateway",
            json!({}),
            &[json!({"api_key": "k", "account_id": "acct1", "gateway_id": "prod"})],
        );
        assert_eq!(out.channel, "custom");
        assert_eq!(
            out.base_url.as_deref(),
            Some("https://api.cloudflare.com/client/v4/accounts/acct1/ai")
        );
        assert_eq!(out.config["headers"]["cf-aig-gateway-id"], json!("prod"));
        assert_eq!(
            out.config["dialects"],
            json!(["openai_chat", "openai", "claude"])
        );
        // And the two fields that moved are taken out of the credential.
        assert_eq!(out.strip_from_secrets, ["account_id", "gateway_id"]);
        assert!(out.note.unwrap().contains("acct1"));
    }

    #[test]
    fn cloudflare_defaults_the_gateway_v3_defaulted() {
        let out = translate(
            "cloudflare-ai-gateway",
            json!({"base_url": "https://api.cloudflare.com/"}),
            &[json!({"api_key": "k", "account_id": "acct1"})],
        );
        assert_eq!(
            out.config["headers"]["cf-aig-gateway-id"],
            json!(CLOUDFLARE_DEFAULT_GATEWAY)
        );
        // The trailing slash does not become a double one.
        assert_eq!(
            out.base_url.as_deref(),
            Some("https://api.cloudflare.com/client/v4/accounts/acct1/ai")
        );
    }

    #[test]
    fn a_cloudflare_provider_without_an_account_stops_the_migration() {
        let error = provider(
            "cloudflare-ai-gateway",
            "cf",
            &json!({}),
            &[json!({"api_key": "k"})],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("provider `cf`"), "{error}");
        assert!(error.contains("account_id"), "{error}");
    }

    /// One v4 provider has one origin, so two accounts is two providers and
    /// this migration will not guess which.
    #[test]
    fn a_cloudflare_provider_spanning_two_accounts_stops_the_migration() {
        let error = provider(
            "cloudflare-ai-gateway",
            "cf",
            &json!({}),
            &[
                json!({"api_key": "a", "account_id": "one"}),
                json!({"api_key": "b", "account_id": "two"}),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("2 different Cloudflare accounts"), "{error}");
        assert!(error.contains("one, two"), "{error}");
    }

    #[test]
    fn an_unknown_channel_fails_loudly_and_names_the_provider() {
        let error = provider("some-fork", "mine", &json!({}), &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("provider `mine`"), "{error}");
        assert!(error.contains("`some-fork`"), "{error}");
        assert!(error.contains("no rule for"), "{error}");
    }
}
