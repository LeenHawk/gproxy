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
        "grokbuild" => oauth(&mut metadata, &[LoginMode::Device], true),
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
        // Account tokens from the device login, or a pasted workspace API key —
        // `prepare` accepts either, so the template offers both.
        "cline" => {
            metadata.credential_family = CredentialFamily::OauthTokens;
            metadata.login_modes = vec![LoginMode::Device];
            metadata.secret_template =
                json!({ "access_token": "", "refresh_token": "", "api_key": "" });
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
            "openai_compact",
        ],
        "aws-bedrock" => &[
            "openai_list_models",
            "openai_get_model",
            "claude_count_tokens",
            "claude_messages",
            "openai_compact",
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
            "openai_audio_speech",
            "openai_audio_transcriptions",
            "openai_audio_translations",
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
            "image_generations",
            "image_edits",
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
            "image_generations",
            "image_edits",
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
        ],
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
