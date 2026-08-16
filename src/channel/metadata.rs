//! Accurate metadata for built-in adapters; external channels self-describe.

use serde_json::json;

use crate::channel::{
    Channel, ChannelMetadata, ChannelSettingField, CredentialFamily, LoginMode, SettingControl,
};

pub fn builtin(channel: &dyn Channel) -> ChannelMetadata {
    let id = channel.id();
    let mut metadata = ChannelMetadata::new(id);
    metadata.display_name = display_name(id).to_string();
    metadata.endpoint_kinds = endpoint_kinds(id)
        .iter()
        .map(|kind| (*kind).into())
        .collect();

    match id {
        "vertex" => {
            metadata.credential_family = CredentialFamily::ServiceAccount;
            metadata.secret_template =
                json!({ "client_email": "", "private_key": "", "project_id": "" });
        }
        "aws-bedrock" => {
            metadata.settings_fields = vec![ChannelSettingField {
                key: "video_output_s3_uri".into(),
                control: SettingControl::Text,
                label: Some("Video output S3 URI".into()),
                required: false,
                default: None,
                placeholder: Some("s3://bucket/prefix".into()),
            }];
        }
        "geminicli" | "antigravity" => {
            metadata.credential_family = CredentialFamily::OauthTokens;
            metadata.login_modes = vec![LoginMode::Authcode];
            metadata.secret_template =
                json!({ "access_token": "", "refresh_token": "", "project_id": "" });
            metadata.usage = true;
        }
        "claudecode" => {
            #[cfg(not(target_arch = "wasm32"))]
            oauth(
                &mut metadata,
                &[LoginMode::Authcode, LoginMode::Cookie],
                true,
            );
            #[cfg(target_arch = "wasm32")]
            oauth(&mut metadata, &[LoginMode::Authcode], true);
        }
        "claudeweb" => {
            oauth(&mut metadata, &[LoginMode::Cookie], true);
            metadata.secret_template = json!({ "cookie": "", "account_uuid": "" });
        }
        "codex" => {
            oauth(
                &mut metadata,
                &[LoginMode::Authcode, LoginMode::Device],
                true,
            );
            metadata.secret_template =
                json!({ "access_token": "", "refresh_token": "", "account_id": "" });
        }
        "cloudflare-ai-gateway" => {
            metadata.secret_template =
                json!({ "api_key": "", "account_id": "", "gateway_id": "default" });
        }
        "grokbuild" => {
            oauth(&mut metadata, &[LoginMode::Device], true);
            metadata.settings_fields = vec![ChannelSettingField {
                key: "xai_api_base_url".into(),
                control: SettingControl::Url,
                label: Some("xAI media API URL".into()),
                required: false,
                default: Some(json!("https://api.x.ai/v1")),
                placeholder: Some("https://api.x.ai/v1".into()),
            }];
        }
        "kimicode" => {
            oauth(&mut metadata, &[LoginMode::Device], true);
            metadata.secret_template =
                json!({ "access_token": "", "refresh_token": "", "device_id": "" });
            metadata.settings_fields = vec![ChannelSettingField {
                key: "oauth_host".into(),
                control: SettingControl::Url,
                label: Some("Kimi OAuth URL".into()),
                required: false,
                default: Some(json!("https://auth.kimi.com")),
                placeholder: Some("https://auth.kimi.com".into()),
            }];
        }
        "workbuddy" => {
            oauth(&mut metadata, &[LoginMode::Device], true);
            metadata.secret_template = json!({
                "access_token": "",
                "refresh_token": "",
                "user_id": "",
                "enterprise_id": "",
                "department_full_name": "",
                "domain": ""
            });
        }
        // The request credential is an API key. Device login is an additional
        // way to obtain one and may retain refresh fields alongside it, just as
        // OpenCode Zen/Go do.
        "cline" => {
            metadata.login_modes = vec![LoginMode::Device];
            metadata.usage = true;
        }
        "kiro" => oauth(
            &mut metadata,
            &[LoginMode::Authcode, LoginMode::Device],
            true,
        ),
        "copilotcli" => {
            metadata.credential_family = CredentialFamily::GithubToken;
            metadata.login_modes = vec![LoginMode::Device];
            metadata.secret_template = json!({ "github_token": "" });
            metadata.usage = true;
        }
        // The gateway credential stays an API key; the console device login is
        // an extra way to obtain one, not a second credential family.
        "opencodezen" | "opencodego" => {
            metadata.login_modes = vec![LoginMode::Device];
            metadata.settings_fields = vec![ChannelSettingField {
                key: "console_base_url".into(),
                control: SettingControl::Url,
                label: Some("OpenCode Console URL".into()),
                required: false,
                default: None,
                placeholder: Some("https://console.opencode.ai".into()),
            }];
        }
        _ => {}
    }
    metadata
}

fn oauth(metadata: &mut ChannelMetadata, login_modes: &[LoginMode], usage: bool) {
    metadata.credential_family = CredentialFamily::OauthTokens;
    metadata.login_modes = login_modes.to_vec();
    metadata.secret_template = json!({ "access_token": "", "refresh_token": "" });
    metadata.usage = usage;
}

fn display_name(id: &str) -> &str {
    match id {
        "aistudio" => "Google AI Studio",
        "antigravity" => "Antigravity",
        "aws-bedrock" => "AWS Bedrock",
        "azure" => "Microsoft Azure",
        "claudeapi" => "Claude API",
        "claudecode" => "Claude Code",
        "claudeweb" => "Claude Web",
        "cline" => "Cline",
        "cloudflare-ai-gateway" => "Cloudflare AI Gateway",
        "codex" => "OpenAI Codex",
        "copilotcli" => "GitHub Copilot CLI",
        "custom" => "Custom",
        "dashscope" => "Alibaba Qwen",
        "deepseek" => "DeepSeek",
        "geminicli" => "Gemini CLI",
        "groq" => "Groq",
        "grokbuild" => "Grok Build",
        "kiro" => "Kiro",
        "kimiapi" => "Kimi API",
        "kimicode" => "Kimi Code",
        "nvidia" => "NVIDIA",
        "openai" => "OpenAI",
        "opencodego" => "OpenCode Go",
        "opencodezen" => "OpenCode Zen",
        "openrouter" => "OpenRouter",
        "vercel" => "Vercel AI Gateway",
        "vertex" => "Google Vertex AI",
        "vertexexpress" => "Vertex AI Express",
        "workbuddy" => "WorkBuddy",
        "xai" => "xAI",
        _ => id,
    }
}

fn endpoint_kinds(id: &str) -> &'static [&'static str] {
    match id {
        "openai" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "openai_embeddings",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_audio_translations",
            "image_generations",
            "image_edits",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_list",
            "openai_video_delete",
            "openai_video_content",
            "openai_video_remix",
            "openai_video_character_create",
            "openai_video_character_get",
            "openai_video_edit",
            "openai_video_extend",
            "openai_compact",
        ],
        "azure" => &[
            "openai_list_models",
            "openai_get_model",
            "claude_count_tokens",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "openai_embeddings",
            "image_generations",
            "image_edits",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_list",
            "openai_video_delete",
            "openai_video_content",
            "openai_video_remix",
            "openai_video_character_create",
            "openai_video_character_get",
            "openai_video_edit",
            "openai_video_extend",
            "openai_compact",
        ],
        "aws-bedrock" => &[
            "openai_list_models",
            "openai_get_model",
            "claude_count_tokens",
            "claude_messages",
            "openai_compact",
            "openai_video_create",
            "openai_video_retrieve",
        ],
        "openrouter" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "openai_embeddings",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_rerank",
            "image_generations",
            "image_edits",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_content",
        ],
        "cloudflare-ai-gateway" => &[
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
        ],
        "dashscope" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "openai_embeddings",
            "openai_rerank",
            "image_generations",
            "image_edits",
            "openai_compact",
        ],
        "deepseek" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
        ],
        "groq" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
        ],
        "kimiapi" => &["openai_list_models", "openai_chat_completions"],
        "kimicode" => &[
            "openai_list_models",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "openai_compact",
            "usage",
        ],
        "nvidia" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_embeddings",
        ],
        "xai" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "image_generations",
            "image_edits",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_edit",
            "openai_video_extend",
            "openai_compact",
        ],
        "vercel" => &[
            "openai_list_models",
            "openai_get_model",
            "claude_count_tokens",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "openai_embeddings",
        ],
        "custom" => &[
            "openai_list_models",
            "claude_list_models",
            "gemini_list_models",
            "openai_get_model",
            "claude_get_model",
            "gemini_get_model",
            "openai_count_tokens",
            "claude_count_tokens",
            "gemini_count_tokens",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "openai_embeddings",
            "gemini_embeddings",
            "openai_rerank",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_audio_translations",
            "image_generations",
            "image_edits",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_list",
            "openai_video_delete",
            "openai_video_content",
            "openai_video_remix",
            "openai_video_character_create",
            "openai_video_character_get",
            "openai_video_edit",
            "openai_video_extend",
            "openai_compact",
        ],
        "claudeapi" => &[
            "openai_list_models",
            "claude_list_models",
            "openai_get_model",
            "claude_get_model",
            "claude_count_tokens",
            "openai_chat_completions",
            "claude_messages",
        ],
        "aistudio" => &[
            "openai_list_models",
            "gemini_list_models",
            "openai_get_model",
            "gemini_get_model",
            "gemini_count_tokens",
            "openai_chat_completions",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "gemini_embeddings",
            "openai_video_create",
            "openai_video_retrieve",
        ],
        "vertex" => &["openai_video_create", "openai_video_retrieve"],
        "vertexexpress" => &[
            "gemini_count_tokens",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "gemini_embeddings",
        ],
        "geminicli" | "antigravity" | "claudeweb" => &["usage"],
        "claudecode" => &[
            "claude_list_models",
            "claude_get_model",
            "claude_count_tokens",
            "claude_messages",
            "usage",
        ],
        "codex" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_responses",
            "image_generations",
            "image_edits",
            "openai_compact",
            "openai_search",
            "openai_realtime_call",
            "usage",
            "rate_limit_reset",
            "rate_limit_reset_credits",
            "account",
            "profile",
            "settings",
            "tasks",
        ],
        "grokbuild" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
            "openai_responses",
            "image_generations",
            "image_edits",
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_video_create",
            "openai_video_retrieve",
            "openai_video_edit",
            "openai_video_extend",
            "openai_compact",
        ],
        "workbuddy" => &[
            "openai_list_models",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "gemini_generate_content",
            "gemini_stream_generate_content",
            "image_generations",
            "image_edits",
            "usage",
        ],
        "kiro" => &["openai_responses"],
        "cline" => &["openai_list_models", "openai_chat_completions", "usage"],
        // Only the surfaces the gateway actually exposes: one OpenAI-shaped
        // catalogue endpoint (the Claude/Gemini lists transform onto it) plus
        // the content surfaces. Get-model and count-tokens are served locally.
        "opencodezen" => &[
            "openai_list_models",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
            "gemini_generate_content",
            "gemini_stream_generate_content",
        ],
        "opencodego" => &[
            "openai_list_models",
            "openai_chat_completions",
            "openai_responses",
            "claude_messages",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use crate::channel::registry::ChannelRegistry;
    use crate::protocol::Operation;

    #[test]
    fn media_endpoint_metadata_matches_routing_tables() {
        let registry = ChannelRegistry::with_builtin();
        let mut mismatches = Vec::new();
        let cases = [
            ("openai_audio_speech", Operation::CreateSpeech),
            (
                "openai_audio_transcriptions",
                Operation::CreateTranscription,
            ),
            ("openai_audio_translations", Operation::CreateTranslation),
            ("image_generations", Operation::CreateImage),
            ("image_edits", Operation::EditImage),
            ("openai_video_create", Operation::CreateVideo),
            ("openai_video_retrieve", Operation::RetrieveVideo),
            ("openai_video_list", Operation::ListVideos),
            ("openai_video_delete", Operation::DeleteVideo),
            ("openai_video_content", Operation::DownloadVideoContent),
            ("openai_video_remix", Operation::RemixVideo),
            (
                "openai_video_character_create",
                Operation::CreateVideoCharacter,
            ),
            ("openai_video_character_get", Operation::GetVideoCharacter),
            ("openai_video_edit", Operation::EditVideo),
            ("openai_video_extend", Operation::ExtendVideo),
        ];

        for entry in registry.catalog() {
            let channel = registry.get(&entry.metadata.id).expect("catalog channel");
            let routes = channel.routing_table();
            for (endpoint, operation) in cases {
                let declared = entry
                    .metadata
                    .endpoint_kinds
                    .iter()
                    .any(|kind| kind == endpoint);
                let routed = routes.iter().any(|(source, decision)| {
                    source.operation() == operation
                        && *decision == crate::routing::RoutingDecision::Passthrough
                });
                if declared != routed {
                    mismatches.push(format!(
                        "{} {endpoint}: declared={declared} routed={routed}",
                        entry.metadata.id
                    ));
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }
}
