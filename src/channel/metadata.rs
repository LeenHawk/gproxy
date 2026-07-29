//! Accurate metadata for built-in adapters; external channels self-describe.

use serde_json::json;

use crate::channel::{Channel, ChannelMetadata, CredentialFamily, LoginMode};

pub fn builtin(channel: &dyn Channel) -> ChannelMetadata {
    let id = channel.id();
    let mut metadata = ChannelMetadata::new(id, channel.provider_family());
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
        "grokbuild" => oauth(&mut metadata, &[LoginMode::Device], true),
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
        "claudeapi" => "Claude API",
        "claudecode" => "Claude Code",
        "claudeweb" => "Claude Web",
        "copilotcli" => "GitHub Copilot CLI",
        "geminicli" => "Gemini CLI",
        "grokbuild" => "Grok Build",
        "vertexexpress" => "Vertex AI Express",
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
        ],
        "deepseek" => &[
            "openai_list_models",
            "openai_get_model",
            "openai_chat_completions",
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
            "usage",
            "rate_limit_reset",
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
        "kiro" => &["openai_responses"],
        _ => &[],
    }
}
