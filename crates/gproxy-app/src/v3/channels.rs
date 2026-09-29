//! v3's channel ids as v4's, which is not a table of renames.
//!
//! # Why this is not a rename table
//!
//! v3 had 27 channels and v4 has 29, and the overlap is not the whole of
//! either. Reading production's twelve providers turned up two distinct
//! problems:
//!
//! 1. **An id v4 spells differently** — `aws-bedrock` is `aws_bedrock` and
//!    `cloudflare-ai-gateway` is `cloudflare_ai_gateway`, a hyphen where v4
//!    writes an underscore. Copying the column verbatim produces a provider
//!    naming a channel that is not registered, which v4 treats as a
//!    configuration error rather than falling back to anything.
//! 2. **`opencode` is two channels in v4**, one per product, where v3 told Zen
//!    from Go by a `tier` setting (see [`opencode`]).
//! 3. Everything else keeps its id, and is checked against the registry rather
//!    than assumed.
//!
//! A channel id that reaches none of those **fails the migration**, naming the
//! provider. The alternative is a row that imports and then cannot serve a
//! request, which is the failure mode this whole migration exists to avoid.
//!
//! v4 once served `cloudflare-ai-gateway`, `nvidia` and `vercel` as `custom`
//! providers, rebuilding each row into an origin and a header. That lost what
//! v3 kept per credential (Cloudflare's account and gateway) and what a row
//! cannot say (Cloudflare's bearer on the Claude surface, NVIDIA's streamed
//! usage opt-in, both balances), so all three are channels again and carry
//! straight across.

use serde_json::Value;

use super::{Error, Result};

/// Every channel id v4 registers, as `gproxy_channel::channels::compiled_in()`
/// declares them. Listed rather than queried because a build with a reduced
/// feature set must still translate a full document: a provider whose channel
/// this binary was not compiled with is a runtime concern the operator can fix
/// by rebuilding, not a reason to refuse their configuration.
pub const V4_CHANNELS: [&str; 29] = [
    "aistudio",
    "antigravity",
    "aws_bedrock",
    "azure",
    "claudeapi",
    "claudecode",
    "claudeweb",
    "cline",
    "cloudflare_ai_gateway",
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
    "nvidia",
    "openai",
    "opencodego",
    "opencodezen",
    "openrouter",
    "vercel",
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
    /// v3 folded OpenCode Zen and Go into one id and told them apart by a
    /// `tier` setting; v4 has a channel for each. See [`opencode`].
    OpenCode,
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
        Rule::Rename("cloudflare_ai_gateway"),
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
    ("nvidia", Rule::Keep),
    ("openai", Rule::Keep),
    ("opencode", Rule::OpenCode),
    ("openrouter", Rule::Keep),
    ("vercel", Rule::Keep),
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

/// One v3 provider as a v4 one: the channel it speaks, the origin and the
/// config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    pub channel: String,
    pub base_url: Option<String>,
    pub config: Value,
    /// What changed, for the report. None when the id carried straight across.
    pub note: Option<String>,
}

/// Translate one provider.
///
/// `name` is only for the error: an operator with twelve providers needs to be
/// told which one stopped the migration.
pub fn provider(v3_channel: &str, name: &str, settings: &Value) -> Result<Provider> {
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
                note: (canonical != id)
                    .then(|| format!("channel `{id}` is v3's old name for `{canonical}`")),
            })
        }
        Rule::Rename(v4) => Ok(Provider {
            channel: (*v4).to_owned(),
            base_url: base_url(settings),
            config: object(settings),
            note: Some(format!("channel `{id}` is `{v4}` in v4")),
        }),
        Rule::OpenCode => Ok(opencode(id, settings)),
    }
}

/// v3's one OpenCode id as v4's two channels. The legacy alias the row was
/// stored under decides first, exactly as v3's own canonicalization forced
/// the tier from it; then v3's `tier` setting (absent meant Zen); then an
/// origin pointed at the Go path, which v3's quota code also read as Go.
fn opencode(id: &str, settings: &Value) -> Provider {
    let origin = base_url(settings);
    let go = match id {
        "opencodego" => true,
        "opencodezen" => false,
        _ => {
            settings.get("tier").and_then(Value::as_str) == Some("go")
                || origin
                    .as_deref()
                    .is_some_and(|url| url.trim_end_matches('/').ends_with("/zen/go/v1"))
        }
    };
    let channel = if go { "opencodego" } else { "opencodezen" };
    let mut config = object(settings);
    // The tier is the channel now; left behind it would be a second answer.
    if let Value::Object(map) = &mut config {
        map.remove("tier");
    }
    Provider {
        channel: channel.to_owned(),
        base_url: origin,
        config,
        note: Some(format!(
            "channel `{id}` is `{channel}` in v4, which has one channel per OpenCode product"
        )),
    }
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
    use serde_json::json;

    fn translate(channel: &str, settings: Value) -> Provider {
        provider(channel, "p", &settings).expect("translates")
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
                Rule::OpenCode => assert!(
                    ["opencodezen", "opencodego"]
                        .iter()
                        .all(|v4| V4_CHANNELS.contains(v4)),
                    "OpenCode splits into channels v4 does not register"
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
        let out = translate("claudecode", json!({"beta": true}));
        assert_eq!(out.channel, "claudecode");
        assert_eq!(out.config, json!({"beta": true}));
        assert!(out.note.is_none());
    }

    /// The one production tripped over.
    #[test]
    fn a_hyphenated_id_takes_v4s_underscore() {
        let out = translate("aws-bedrock", json!({"region": "us-east-1"}));
        assert_eq!(out.channel, "aws_bedrock");
        assert_eq!(out.config, json!({"region": "us-east-1"}));
        assert!(out.note.unwrap().contains("`aws_bedrock` in v4"));

        // The account and gateway stay in the credential, as v3 kept them.
        let out = translate("cloudflare-ai-gateway", json!({}));
        assert_eq!(out.channel, "cloudflare_ai_gateway");
        assert_eq!(out.config, json!({}));
    }

    #[test]
    fn v3s_own_legacy_aliases_still_resolve() {
        for (alias, target) in ALIASES {
            let out = translate(alias, json!({}));
            match target {
                "opencode" => assert_eq!(out.channel, alias, "the alias names its product"),
                _ => {
                    assert_eq!(out.channel, target);
                    assert!(out.note.unwrap().contains("old name"));
                }
            }
        }
    }

    #[test]
    fn opencode_splits_into_the_product_v3_served() {
        let zen = translate("opencode", json!({"future": true}));
        assert_eq!(zen.channel, "opencodezen");
        assert_eq!(zen.config, json!({"future": true}));

        let go = translate("opencode", json!({"tier": "go", "future": true}));
        assert_eq!(go.channel, "opencodego");
        assert_eq!(
            go.config,
            json!({"future": true}),
            "the tier is the channel now"
        );

        let by_origin = translate(
            "opencode",
            json!({"base_url": "https://opencode.ai/zen/go/v1/"}),
        );
        assert_eq!(by_origin.channel, "opencodego");
        assert_eq!(
            by_origin.base_url.as_deref(),
            Some("https://opencode.ai/zen/go/v1/")
        );

        // v3 forced the tier from the alias, whatever the setting said.
        let aliased = translate("opencodego", json!({"tier": "zen"}));
        assert_eq!(aliased.channel, "opencodego");
    }

    #[test]
    fn a_base_url_inside_v3s_settings_becomes_v4s_column() {
        let out = translate("custom", json!({"base_url": " https://x.invalid "}));
        assert_eq!(out.base_url.as_deref(), Some("https://x.invalid"));
        // And it stays in `config` too, because a channel may still read it.
        assert_eq!(out.config["base_url"], json!(" https://x.invalid "));
    }

    #[test]
    fn a_null_settings_column_becomes_the_object_v4_requires() {
        assert_eq!(translate("openai", Value::Null).config, json!({}));
    }

    #[test]
    fn an_unknown_channel_fails_loudly_and_names_the_provider() {
        let error = provider("some-fork", "mine", &json!({}))
            .unwrap_err()
            .to_string();
        assert!(error.contains("provider `mine`"), "{error}");
        assert!(error.contains("`some-fork`"), "{error}");
        assert!(error.contains("no rule for"), "{error}");
    }
}
