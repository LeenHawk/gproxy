//! v3's per-provider `endpoints` settings as v4's `operation_endpoints`.
//!
//! v3 let a provider replace the URL of one operation with
//! `settings.endpoints.{name}: url`, the name being v3's own for the operation
//! and wire (`v3:crates/gproxy-channel-api/src/endpoint.rs`). v4 keeps the
//! same idea as a row per `(operation, dialect, transport)`. Left in `config`
//! the object is read by nothing, so an overriding provider would quietly go
//! back to the channel's default path; it is translated and then removed.
//!
//! `claudeweb` is the exception that still reads `config.endpoints` in v4,
//! for its own `claudeweb_*` names; those stay where they are.
//!
//! Three things do not carry, and each is reported rather than guessed:
//! - a URL with v3's `{model}` placeholder, because v4 fills in no
//!   placeholders and the literal text would reach the upstream;
//! - a name for an operation v4 does not have (Sora's remix, edit, extend
//!   and characters) or one this table does not know;
//! - a value that is not an absolute http(s) or ws(s) URL, which v4's
//!   assembly would refuse, failing the whole snapshot.

use gproxy_sdk::dto::OperationEndpointDto;
use serde_json::Value;

use super::{Report, ids};

const HTTP: &str = "http";
const WEBSOCKET: &str = "websocket";

/// Content generation names cover the call and its streaming form, as v3's
/// key did (it was keyed on the wire, not the operation).
const GENERATE: &[&str] = &["generate_content", "stream_generate_content"];

/// v3's endpoint name → the v4 operations, dialect and transport it meant.
fn target(name: &str) -> Option<(&'static [&'static str], &'static str, &'static str)> {
    Some(match name {
        "openai_chat_completions" => (GENERATE, "openai_chat", HTTP),
        "openai_responses" => (GENERATE, "openai", HTTP),
        "claude_messages" => (GENERATE, "claude", HTTP),
        "gemini_generate_content" => (&["generate_content"], "gemini", HTTP),
        "gemini_stream_generate_content" => (&["stream_generate_content"], "gemini", HTTP),
        "openai_responses_websocket" => (GENERATE, "openai_responses_websocket", WEBSOCKET),
        "openai_realtime" => (&["connect_realtime"], "openai", WEBSOCKET),
        "openai_list_models" => (&["list_models"], "openai", HTTP),
        "claude_list_models" => (&["list_models"], "claude", HTTP),
        "gemini_list_models" => (&["list_models"], "gemini", HTTP),
        "openai_get_model" => (&["get_model"], "openai", HTTP),
        "claude_get_model" => (&["get_model"], "claude", HTTP),
        "gemini_get_model" => (&["get_model"], "gemini", HTTP),
        "openai_count_tokens" => (&["count_tokens"], "openai", HTTP),
        "claude_count_tokens" => (&["count_tokens"], "claude", HTTP),
        "gemini_count_tokens" => (&["count_tokens"], "gemini", HTTP),
        "openai_embeddings" => (&["create_embedding"], "openai", HTTP),
        "gemini_embeddings" => (&["create_embedding"], "gemini", HTTP),
        "openai_rerank" => (&["rerank"], "openai", HTTP),
        "image_generations" => (&["create_image"], "openai", HTTP),
        "image_edits" => (&["edit_image"], "openai", HTTP),
        "openai_audio_speech" => (&["create_speech"], "openai", HTTP),
        "openai_audio_transcriptions" => (&["create_transcription"], "openai", HTTP),
        "openai_audio_translations" => (&["create_translation"], "openai", HTTP),
        "openai_compact" => (&["compact_content"], "openai", HTTP),
        "openai_conversations" => (&["create_conversation"], "openai", HTTP),
        "openai_file_create" => (&["create_file"], "openai", HTTP),
        "openai_file_list" => (&["list_files"], "openai", HTTP),
        "openai_file_retrieve" => (&["retrieve_file"], "openai", HTTP),
        "openai_file_content" => (&["retrieve_file_content"], "openai", HTTP),
        "openai_file_delete" => (&["delete_file"], "openai", HTTP),
        "openai_video_create" => (&["create_video"], "openai", HTTP),
        "openai_video_retrieve" => (&["retrieve_video"], "openai", HTTP),
        "openai_video_list" => (&["list_videos"], "openai", HTTP),
        "openai_video_delete" => (&["delete_video"], "openai", HTTP),
        "openai_video_content" => (&["download_video_content"], "openai", HTTP),
        _ => return None,
    })
}

/// v4 stores an http(s) URL for a socket too and lets the channel switch the
/// scheme at the handshake, so a v3 `wss://` override is written as `https://`.
fn url_for(raw: &str, transport: &str) -> Option<String> {
    let url = raw.trim();
    let url = if transport == WEBSOCKET {
        if let Some(rest) = url.strip_prefix("wss://") {
            format!("https://{rest}")
        } else if let Some(rest) = url.strip_prefix("ws://") {
            format!("http://{rest}")
        } else {
            url.to_owned()
        }
    } else {
        url.to_owned()
    };
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

/// v4 channels that still read their own names out of `config.endpoints`.
const READS_CONFIG_ENDPOINTS: &[&str] = &["claudeweb"];

/// Take the operation overrides out of a provider's translated `config` and
/// return the rows they become.
pub fn translate(
    provider_id: i64,
    provider_name: &str,
    channel: &str,
    config: &mut Value,
    report: &mut Report,
) -> Vec<OperationEndpointDto> {
    let Some(Value::Object(endpoints)) = config
        .as_object_mut()
        .and_then(|config| config.remove("endpoints"))
    else {
        return Vec::new();
    };
    let v4_id = ids::id("providers", provider_id);
    let keeps_its_own = READS_CONFIG_ENDPOINTS.contains(&channel);
    let mut kept = serde_json::Map::new();
    let mut rows = Vec::new();
    for (name, value) in endpoints {
        let row = format!("provider {provider_id} ({provider_name}), endpoint `{name}`");
        let Some((operations, dialect, transport)) = target(&name) else {
            if keeps_its_own {
                kept.insert(name, value);
                continue;
            }
            report.drop_row(
                "endpoints",
                row,
                "v4 has no operation this v3 endpoint name maps to".to_owned(),
            );
            continue;
        };
        let Some(raw) = value.as_str().map(str::trim).filter(|url| !url.is_empty()) else {
            continue;
        };
        if raw.contains("{model}") {
            report.drop_row(
                "endpoints",
                row,
                format!(
                    "`{raw}` uses v3's `{{model}}` placeholder, which v4 does not fill in; \
                     set a fixed URL per model on the provider instead"
                ),
            );
            continue;
        }
        let Some(url) = url_for(raw, transport) else {
            report.drop_row(
                "endpoints",
                row,
                format!("`{raw}` is not an absolute http(s) URL"),
            );
            continue;
        };
        for operation in operations {
            rows.push(OperationEndpointDto {
                id: ids::part(
                    "providers",
                    provider_id,
                    &format!("endpoint-{name}-{operation}"),
                ),
                provider_id: v4_id.clone(),
                operation: (*operation).to_owned(),
                dialect: dialect.to_owned(),
                transport: transport.to_owned(),
                url: url.clone(),
                enabled: true,
            });
        }
    }
    if !kept.is_empty()
        && let Some(config) = config.as_object_mut()
    {
        config.insert("endpoints".into(), Value::Object(kept));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_sdk::{Dialect, Operation};
    use serde_json::json;

    #[test]
    fn every_target_names_an_operation_and_dialect_v4_parses() {
        let names = [
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "openai_responses_websocket",
            "openai_realtime",
            "openai_list_models",
            "claude_list_models",
            "gemini_list_models",
            "openai_get_model",
            "claude_get_model",
            "gemini_get_model",
            "openai_count_tokens",
            "claude_count_tokens",
            "gemini_count_tokens",
            "openai_embeddings",
            "gemini_embeddings",
            "openai_rerank",
            "image_generations",
            "image_edits",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_audio_translations",
            "openai_compact",
            "openai_conversations",
            "openai_file_create",
            "openai_file_list",
            "openai_file_retrieve",
            "openai_file_content",
            "openai_file_delete",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_list",
            "openai_video_delete",
            "openai_video_content",
        ];
        for name in names {
            let (operations, dialect, _) = target(name).expect(name);
            assert!(Dialect::from_id(dialect).is_some(), "{name}: {dialect}");
            for operation in operations {
                assert!(
                    Operation::from_id(operation).is_some(),
                    "{name}: {operation}"
                );
            }
        }
    }

    #[test]
    fn endpoints_become_rows_and_leave_the_config() {
        let mut config = json!({"headers": {}, "endpoints": {
            "openai_chat_completions": "https://a.example/chat",
            "openai_realtime": "wss://a.example/realtime",
            "gemini_generate_content": "https://a.example/models/{model}:generateContent",
            "openai_video_remix": "https://a.example/remix",
            "claude_messages": "",
        }});
        let mut report = Report::default();
        let rows = translate(7, "p", "openai", &mut config, &mut report);
        assert_eq!(config, json!({"headers": {}}));
        let mut got: Vec<_> = rows
            .iter()
            .map(|r| {
                (
                    r.operation.as_str(),
                    r.dialect.as_str(),
                    r.transport.as_str(),
                    r.url.as_str(),
                )
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            [
                (
                    "connect_realtime",
                    "openai",
                    "websocket",
                    "https://a.example/realtime"
                ),
                (
                    "generate_content",
                    "openai_chat",
                    "http",
                    "https://a.example/chat"
                ),
                (
                    "stream_generate_content",
                    "openai_chat",
                    "http",
                    "https://a.example/chat"
                ),
            ]
        );
        assert!(rows.iter().all(|r| r.provider_id == "v3-providers-7"));
        assert_eq!(report.dropped.len(), 2, "{:?}", report.dropped);

        // claudeweb still reads its own names from config.
        let mut config = json!({"endpoints": {
            "claudeweb_bootstrap": "https://c.example/bootstrap",
            "openai_chat_completions": "https://c.example/chat",
        }});
        let rows = translate(8, "w", "claudeweb", &mut config, &mut report);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            config,
            json!({"endpoints": {"claudeweb_bootstrap": "https://c.example/bootstrap"}})
        );
    }
}
