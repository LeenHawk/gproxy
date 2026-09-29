//! v3 provider settings v4 reads under another name or in another shape.
//!
//! Migration copies `settings` into `config` (see [`super::channels`]), and
//! every v4 channel ignores keys it does not know, so a key v4 renamed is not
//! an error but a setting quietly switched off. These are the ones found by
//! reading both sides; each is translated where v4 can say the same thing and
//! reported where it cannot.
//!
//! - `credential_strategy` was a provider **column** in v3 and is a `config`
//!   key in v4. v3 had `round_robin` and `sticky`, both v4 values.
//! - `claude_fallback_mode` / `claude_fallback_models` are `fallback_mode` /
//!   `fallback_models` to the channels that place the fallback themselves
//!   (claudeapi, claudecode, custom, openrouter). v3's older
//!   `claude_fable_fallbacks`, read when no mode was set, becomes the mode it
//!   meant: `"default"`, or `models` with its list.
//! - OpenCode read the console from `oauth_host` when `console_base_url` was
//!   absent; v4 reads only the latter.
//! - Kimi chose its product from the **credential**: an OAuth login reached
//!   Kimi Code, a key the Moonshot platform. v4 chooses per provider, from
//!   `product` or the origin, and defaults to the platform, so a login-only
//!   provider with no origin would be sent to the wrong upstream.
//! - The Google login channels (`antigravity`, `geminicli`) read the token
//!   endpoint from `token_url` where v3 read `oauth_token_url`. v3's
//!   `oauth_client_id` / `oauth_client_secret` have no v4 form — v4 presents
//!   the tool's own client — and are reported, since a refresh token minted
//!   for another client will not refresh under this one.
//! - One v3 `base_url` served two hosts where v4 has two keys: Bedrock's
//!   runtime and control plane (`control_base_url`), Grok Build's CLI and
//!   media hosts (`media_base_url`). The second key gets the same origin.
//! - Settings for features the v4 channel does not have are reported:
//!   OpenCode's Console balance source, Grok Build's own OAuth client,
//!   and Bedrock's video output bucket. Azure's `api_version` is warned about when no deployment is
//!   set: v3 sent it only with image calls, v4 with every call.
//! - Codex's `codex_pat_plan_type` moves onto each credential
//!   (`config::credential_metadata`) and leaves the provider.
//! - `traffic_policy.request_headers` is v4's `allowed_headers` when it lists
//!   plain names. v3's patterns (`*`, `x-foo-*`), `response_headers` and
//!   `request_query` have no v4 form.

use serde_json::{Map, Value, json};

use super::Report;

/// What a provider's credentials tell about it, which the Kimi rule needs.
#[derive(Debug, Default, Clone, Copy)]
pub struct Credentials {
    pub oauth: bool,
    pub api_key: bool,
}

/// The provider the settings belong to, as far as these rules need it.
pub struct Provider<'a> {
    pub id: i64,
    pub name: &'a str,
    /// The v4 channel it became.
    pub channel: &'a str,
    pub base_url: Option<&'a str>,
    /// v3's column, not a settings key.
    pub credential_strategy: Option<&'a str>,
    pub credentials: Credentials,
}

pub fn translate(provider: &Provider<'_>, config: &mut Value, report: &mut Report) {
    let Provider {
        id: provider_id,
        name: provider_name,
        channel,
        base_url,
        credential_strategy,
        credentials,
    } = *provider;
    let Some(map) = config.as_object_mut() else {
        return;
    };
    let mut warn = |message: String| {
        report.warn(format!(
            "provider {provider_id} ({provider_name}): {message}"
        ));
    };

    match credential_strategy.map(str::trim) {
        None | Some("") => {}
        Some(strategy @ ("round_robin" | "sticky")) => {
            map.entry("credential_strategy")
                .or_insert_with(|| json!(strategy));
        }
        Some(other) => warn(format!(
            "credential strategy `{other}` is not one v4 has; it uses round_robin"
        )),
    }

    fallback(map);

    if channel == "opencodezen"
        && !map.contains_key("console_base_url")
        && let Some(host) = map.remove("oauth_host")
    {
        map.insert("console_base_url".into(), host);
    }

    if channel == "kimi" && !map.contains_key("product") {
        let origin = base_url.unwrap_or_default();
        let origin_says_code = origin.contains("api.kimi.com") || origin.contains("/coding");
        if !origin_says_code && credentials.oauth {
            if credentials.api_key {
                warn(
                    "v3 served its OAuth credentials through Kimi Code and its keys through \
                     the Moonshot platform; v4 picks one product per provider, so it was set \
                     to `code` and the keys need a provider of their own"
                        .to_owned(),
                );
            }
            map.insert("product".into(), json!("code"));
        }
    }

    if matches!(channel, "antigravity" | "geminicli") {
        if let Some(url) = map.remove("oauth_token_url") {
            map.entry("token_url").or_insert(url);
        }
        for key in ["oauth_client_id", "oauth_client_secret"] {
            if map.remove(key).is_some() {
                warn(format!(
                    "{key} has no v4 setting: v4 refreshes with the tool's own client, so a \
                     credential logged in under another client has to log in again"
                ));
            }
        }
    }
    if channel == "codex" {
        map.remove("codex_pat_plan_type");
    }
    let second_host = match channel {
        "aws_bedrock" => Some("control_base_url"),
        "grokbuild" => Some("media_base_url"),
        _ => None,
    };
    if let (Some(key), Some(origin)) = (second_host, base_url.filter(|url| !url.trim().is_empty()))
    {
        map.entry(key).or_insert_with(|| json!(origin));
    }
    let unsupported: &[&str] = match channel {
        "opencodezen" | "opencodego" => &["quota_workspace_id", "quota_base_url", "quota_cookie"],
        "grokbuild" => &["oauth_client_id"],
        "aws_bedrock" => &["video_output_s3_uri"],
        _ => &[],
    };
    for key in unsupported {
        if map
            .remove(*key)
            .is_some_and(|value| !value.is_null() && value != Value::Bool(false))
        {
            warn(format!(
                "{key} is a v3 feature the v4 {channel} channel does not have"
            ));
        }
    }
    if channel == "azure"
        && map.get("api_version").is_some_and(|v| !v.is_null())
        && map.get("deployment").is_none_or(Value::is_null)
    {
        warn(
            "api_version was sent only with image calls in v3; v4 sends it with every call, \
             so check that the chat and responses surfaces accept it"
                .to_owned(),
        );
    }

    traffic_policy(map, &mut warn);
}

fn fallback(map: &mut Map<String, Value>) {
    let legacy = map.remove("claude_fable_fallbacks");
    let mode = map.remove("claude_fallback_mode");
    let models = map.remove("claude_fallback_models");
    if map.contains_key("fallback_mode") {
        return;
    }
    match (mode, legacy) {
        (Some(mode), _) => {
            map.insert("fallback_mode".into(), mode);
            if let Some(models) = models {
                map.entry("fallback_models").or_insert(models);
            }
        }
        (None, Some(Value::String(default))) if default == "default" => {
            map.insert("fallback_mode".into(), json!("default"));
        }
        (None, Some(Value::Array(list))) => {
            map.insert("fallback_mode".into(), json!("models"));
            map.entry("fallback_models").or_insert(Value::Array(list));
        }
        _ => {}
    }
}

fn traffic_policy(map: &mut Map<String, Value>, warn: &mut impl FnMut(String)) {
    let Some(policy) = map.remove("traffic_policy") else {
        return;
    };
    let list = |key: &str| -> Vec<String> {
        policy
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.trim().to_ascii_lowercase())
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    let requested = list("request_headers");
    if requested.iter().any(|name| name.contains('*')) {
        warn(format!(
            "traffic_policy.request_headers uses patterns ({}), which v4's allowed_headers \
             cannot express; v4 forwards every header it does not drop by default",
            requested.join(", ")
        ));
    } else if !requested.is_empty() && !map.contains_key("allowed_headers") {
        map.insert("allowed_headers".into(), json!(requested));
    }
    for key in ["response_headers", "request_query"] {
        let entries = list(key);
        if !entries.is_empty() {
            warn(format!(
                "traffic_policy.{key} ({}) has no v4 equivalent and was not carried",
                entries.join(", ")
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(
        channel: &str,
        base: Option<&str>,
        strategy: Option<&str>,
        creds: Credentials,
        config: Value,
    ) -> (Value, Report) {
        let mut config = config;
        let mut report = Report::default();
        let provider = Provider {
            id: 1,
            name: "p",
            channel,
            base_url: base,
            credential_strategy: strategy,
            credentials: creds,
        };
        translate(&provider, &mut config, &mut report);
        (config, report)
    }

    #[test]
    fn renamed_and_relocated_keys_arrive_under_v4s_names() {
        let (config, _) = run(
            "claudeapi",
            None,
            Some("sticky"),
            Credentials::default(),
            json!({"claude_fallback_mode": "models", "claude_fallback_models": ["claude-sonnet-4-5"]}),
        );
        assert_eq!(
            config,
            json!({"credential_strategy": "sticky", "fallback_mode": "models",
                   "fallback_models": ["claude-sonnet-4-5"]})
        );

        let (config, _) = run(
            "custom",
            None,
            None,
            Credentials::default(),
            json!({"claude_fable_fallbacks": ["a", "b"]}),
        );
        assert_eq!(
            config,
            json!({"fallback_mode": "models", "fallback_models": ["a", "b"]})
        );

        let (config, _) = run(
            "opencodezen",
            None,
            None,
            Credentials::default(),
            json!({"oauth_host": "https://console.example"}),
        );
        assert_eq!(
            config,
            json!({"console_base_url": "https://console.example"})
        );
    }

    #[test]
    fn a_shared_origin_and_renamed_client_keys_follow_v4s_layout() {
        let (config, _) = run(
            "aws_bedrock",
            Some("https://proxy.example"),
            None,
            Credentials::default(),
            json!({}),
        );
        assert_eq!(config, json!({"control_base_url": "https://proxy.example"}));
        let (config, report) = run(
            "geminicli",
            None,
            None,
            Credentials::default(),
            json!({"oauth_token_url": "https://t.example", "oauth_client_id": "x"}),
        );
        assert_eq!(config, json!({"token_url": "https://t.example"}));
        assert_eq!(report.warnings.len(), 1);
        let (config, report) = run(
            "azure",
            None,
            None,
            Credentials::default(),
            json!({"api_version": "preview", "enable_openai_magic_cache": true}),
        );
        assert_eq!(
            config,
            json!({"api_version": "preview", "enable_openai_magic_cache": true}),
            "Azure places both kinds now"
        );
        assert_eq!(report.warnings.len(), 1, "the api_version scope");
    }

    #[test]
    fn a_login_only_kimi_provider_is_kimi_code() {
        let oauth = Credentials {
            oauth: true,
            api_key: false,
        };
        assert_eq!(
            run("kimi", None, None, oauth, json!({})).0,
            json!({"product": "code"})
        );
        // The origin already says so, or the operator did.
        assert_eq!(
            run(
                "kimi",
                Some("https://api.kimi.com/coding/v1"),
                None,
                oauth,
                json!({})
            )
            .0,
            json!({})
        );
        assert_eq!(
            run("kimi", None, None, oauth, json!({"product": "platform"})).0,
            json!({"product": "platform"})
        );
        let keys = Credentials {
            oauth: false,
            api_key: true,
        };
        assert_eq!(run("kimi", None, None, keys, json!({})).0, json!({}));
        let mixed = Credentials {
            oauth: true,
            api_key: true,
        };
        let (config, report) = run("kimi", None, None, mixed, json!({}));
        assert_eq!(config, json!({"product": "code"}));
        assert_eq!(report.warnings.len(), 1);
    }

    #[test]
    fn plain_request_headers_become_the_allow_list_and_the_rest_is_reported() {
        let (config, report) = run(
            "openai",
            None,
            None,
            Credentials::default(),
            json!({
            "traffic_policy": {"request_headers": ["X-Trace"], "response_headers": ["x-a"],
                               "request_query": []}}),
        );
        assert_eq!(config, json!({"allowed_headers": ["x-trace"]}));
        assert_eq!(report.warnings.len(), 1);
        let (config, report) = run(
            "openai",
            None,
            None,
            Credentials::default(),
            json!({
            "traffic_policy": {"request_headers": ["x-*"]}}),
        );
        assert_eq!(config, json!({}));
        assert_eq!(report.warnings.len(), 1);
    }
}
