//! The built-in model table: which wire each model is served on, the
//! `x-api-provider` the CLI names for it, and its billing pool.
//!
//! Every row was observed: droid 1.18.0 was run once per built-in model
//! against a loopback recorder (`samples/droid-1.18.0/routes.tsv`), and the
//! billing pool is the model's `billingPool` in the registry
//! (`x/js/0073.js`). The wire is per model, not per vendor: `minimax-m2.7`
//! goes to Claude Messages and `muse-spark-1.3` to Responses.

use gproxy_protocol::Dialect;
use serde_json::{Value, json};

/// One of the four inference paths under `/api/llm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// `/api/llm/a/v1/messages`
    Claude,
    /// `/api/llm/o/v1/responses`
    Responses,
    /// `/api/llm/o/v1/chat/completions`
    Chat,
    /// `/api/llm/g/v1/generate`, always answered as SSE.
    Gemini,
}

impl Wire {
    pub const fn dialect(self) -> Dialect {
        match self {
            Self::Claude => Dialect::Claude,
            Self::Responses => Dialect::OpenAi,
            Self::Chat => Dialect::OpenAiChat,
            Self::Gemini => Dialect::Gemini,
        }
    }
}

pub struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub wire: Wire,
    pub api_provider: &'static str,
    /// Billed to the Droid Core pool rather than Standard.
    pub core: bool,
}

const fn model(
    id: &'static str,
    name: &'static str,
    wire: Wire,
    api_provider: &'static str,
    core: bool,
) -> Model {
    Model {
        id,
        name,
        wire,
        api_provider,
        core,
    }
}

pub const MODELS: &[Model] = &[
    model("claude-sonnet-4-5-20250929", "Sonnet 4.5", Wire::Claude, "anthropic", false),
    model("claude-opus-4-5-20251101", "Opus 4.5", Wire::Claude, "anthropic", false),
    model("claude-sonnet-4-6", "Sonnet 4.6", Wire::Claude, "anthropic", false),
    model("claude-sonnet-5", "Sonnet 5", Wire::Claude, "anthropic", false),
    model("claude-opus-4-6", "Opus 4.6", Wire::Claude, "anthropic", false),
    model("claude-opus-4-7", "Opus 4.7", Wire::Claude, "anthropic", false),
    model("claude-opus-4-8", "Opus 4.8", Wire::Claude, "anthropic", false),
    model("claude-opus-4-8-fast", "Opus 4.8 Fast Mode", Wire::Claude, "anthropic", false),
    model("claude-opus-5", "Opus 5", Wire::Claude, "anthropic", false),
    model("claude-opus-5-fast", "Opus 5 Fast Mode", Wire::Claude, "anthropic", false),
    model("claude-opus-5-5", "Opus 5.5", Wire::Claude, "anthropic", false),
    model("claude-opus-5-5-fast", "Opus 5.5 Fast Mode", Wire::Claude, "anthropic", false),
    model("claude-fable-5", "Fable 5", Wire::Claude, "anthropic", false),
    model("claude-fable-5.1", "Fable 5.1", Wire::Claude, "anthropic", false),
    model("claude-haiku-4-5-20251001", "Haiku 4.5", Wire::Claude, "anthropic", false),
    model("gpt-5.2", "GPT-5.2", Wire::Responses, "openai", false),
    model("gpt-5.3-codex", "GPT-5.3-Codex", Wire::Responses, "openai", false),
    model("gpt-5.3-codex-fast", "GPT-5.3-Codex Fast Mode", Wire::Responses, "openai", false),
    model("gpt-5.4", "GPT-5.4", Wire::Responses, "openai", false),
    model("gpt-5.4-fast", "GPT-5.4 Fast Mode", Wire::Responses, "openai", false),
    model("gpt-5.4-mini", "GPT-5.4 Mini", Wire::Responses, "openai", false),
    model("gpt-5.4-mini-fast", "GPT-5.4 Mini Fast Mode", Wire::Responses, "openai", false),
    model("gpt-5.5", "GPT-5.5", Wire::Responses, "openai", false),
    model("gpt-5.5-fast", "GPT-5.5 Fast Mode", Wire::Responses, "openai", false),
    model("gpt-5.5-pro", "GPT-5.5 Pro", Wire::Responses, "openai", false),
    model("gpt-5.6-sol", "GPT-5.6 Sol", Wire::Responses, "openai", false),
    model("gpt-5.6-sol-fast", "GPT-5.6 Sol Fast Mode", Wire::Responses, "openai", false),
    model("gpt-5.6-terra", "GPT-5.6 Terra", Wire::Responses, "openai", false),
    model("gpt-5.6-luna", "GPT-5.6 Luna", Wire::Responses, "openai", false),
    model("gpt-6-astra", "GPT-6 Astra", Wire::Responses, "openai", false),
    model("gpt-6.1-sol", "GPT-6.1 Sol", Wire::Responses, "openai", false),
    model("gpt-6.1-sol-ultrafast", "GPT-6.1 Sol Ultra Fast", Wire::Responses, "openai", false),
    model("gpt-6-sol", "GPT-6 Sol", Wire::Responses, "openai", false),
    model("gpt-6-sol-fast", "GPT-6 Sol Fast Mode", Wire::Responses, "openai", false),
    model("gpt-6-luna", "GPT-6 Luna", Wire::Responses, "openai", false),
    model("gpt-6-luna-fast", "GPT-6 Luna Fast Mode", Wire::Responses, "openai", false),
    model("gemini-3.1-pro-preview", "Gemini 3.1 Pro", Wire::Gemini, "google", false),
    model("gemini-3-flash-preview", "Gemini 3 Flash", Wire::Gemini, "google", false),
    model("gemini-3.5-flash", "Gemini 3.5 Flash", Wire::Gemini, "google", false),
    model("gemini-3.6-flash", "Gemini 3.6 Flash", Wire::Gemini, "google", false),
    model("gemini-3.7-flash", "Gemini 3.7 Flash", Wire::Gemini, "google", false),
    model("gemini-3.8-flash", "Gemini 3.8 Flash", Wire::Gemini, "google", false),
    model("grok-4.5", "Grok 4.5", Wire::Responses, "xai", false),
    model("grok-4.6", "Grok 4.6", Wire::Responses, "xai", false),
    model("grok-4.7", "Grok 4.7", Wire::Responses, "xai", false),
    model("kimi-k3", "Kimi K3", Wire::Chat, "fireworks", true),
    model("qwen3.8-max", "Qwen3.8 Max", Wire::Chat, "fireworks", true),
    model("deepseek-v4-flash-0731", "DeepSeek V4 Flash 0731", Wire::Chat, "fireworks", true),
    model("deepseek-v4.1-flash", "DeepSeek V4.1 Flash", Wire::Chat, "fireworks", true),
    model("deepseek-v4-pro", "DeepSeek V4 Pro", Wire::Chat, "fireworks", true),
    model("minimax-m2.7", "MiniMax M2.7", Wire::Claude, "fireworks", true),
    model("minimax-m3", "MiniMax M3", Wire::Chat, "fireworks", true),
    model("mistral-medium-3.5", "Mistral Medium 3.5", Wire::Chat, "mistral", true),
    model("glm-5.2", "GLM-5.2", Wire::Chat, "baseten", true),
    model("glm-5.3", "GLM-5.3", Wire::Chat, "fireworks", true),
    model("glm-5.3-flash", "GLM-5.3-Flash", Wire::Chat, "fireworks", true),
    model("nemotron-3-ultra", "Nemotron 3 Ultra", Wire::Chat, "baseten", true),
    model("muse-spark-1.3", "Muse Spark 1.3", Wire::Responses, "google", false),
    model("muse-spark-1.2", "Muse Spark 1.2", Wire::Chat, "fireworks", false),
    model("inkling", "Inkling", Wire::Chat, "fireworks", true),
];

pub fn find(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.id == id)
}

/// The wire for a model. A model the table does not know is placed by its
/// family name, the way the registry groups them; anything else is an open
/// model on Chat Completions.
pub fn wire(id: Option<&str>) -> Wire {
    let Some(id) = id else {
        return Wire::Chat;
    };
    if let Some(model) = find(id) {
        return model.wire;
    }
    if id.starts_with("claude-") {
        Wire::Claude
    } else if id.starts_with("gpt-") || id.starts_with("grok-") {
        Wire::Responses
    } else if id.starts_with("gemini-") {
        Wire::Gemini
    } else {
        Wire::Chat
    }
}

/// The `x-api-provider` the CLI sends for a model it does not list.
pub fn api_provider(id: &str) -> &'static str {
    if let Some(model) = find(id) {
        return model.api_provider;
    }
    match wire(Some(id)) {
        Wire::Claude => "anthropic",
        Wire::Gemini => "google",
        Wire::Responses if id.starts_with("grok-") => "xai",
        Wire::Responses => "openai",
        Wire::Chat => "fireworks",
    }
}

/// Droid Core model ids, the pool the `core` quota windows count.
pub fn core_ids() -> Vec<String> {
    MODELS
        .iter()
        .filter(|model| model.core)
        .map(|model| model.id.to_owned())
        .collect()
}

/// The table as an OpenAI model list; Factory publishes no catalogue.
pub fn list() -> Value {
    json!({
        "object": "list",
        "data": MODELS
            .iter()
            .map(|model| json!({
                "id": model.id,
                "object": "model",
                "created": 0,
                "owned_by": model.api_provider,
                "display_name": model.name,
            }))
            .collect::<Vec<_>>(),
    })
}
