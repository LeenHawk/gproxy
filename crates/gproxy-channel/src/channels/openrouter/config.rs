//! Provider configuration and upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub const ID: &str = "openrouter";
/// The origin already carries `/api`, so a native `/v1/...` path joins onto
/// `https://openrouter.ai/api/v1/...`.
pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct OpenRouterConfig {
    pub fallback_mode: crate::channels::shared::claude_fallback::FallbackMode,
    pub fallback_models: Vec<String>,
    /// `HTTP-Referer` sent when the client did not send its own. OpenRouter
    /// uses it, with `title`, to attribute traffic on its app leaderboard.
    pub referer: Option<String>,
    /// `X-Title` sent when the client did not send its own.
    pub title: Option<String>,
    /// Default provider routing preferences (`order`, `only`, `ignore`,
    /// `allow_fallbacks`, `sort`, `require_parameters`, `data_collection`,
    /// `quantizations`, `max_price`), merged into the request body as
    /// `provider`. A `provider` the client already sent always wins.
    pub provider: Option<Value>,
    /// Routing preferences per upstream model name, overriding `provider`
    /// whole for the models they name.
    pub model_providers: BTreeMap<String, Value>,
    /// Ask for usage accounting (`usage.include`) so the reply carries the
    /// price OpenRouter charged. On by default: without it there is no
    /// `cost` field and the exchange can only be priced from local rates.
    pub usage_accounting: bool,
    /// Rewrite OpenAI's `service_tier: "fast"`, which OpenRouter rejects,
    /// into its own `"priority"`.
    pub normalize_service_tier: bool,
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
    /// Place `cache_control` where a client embeds a magic cache string in a
    /// Claude-dialect body (`channels::shared::cache`). Off by default; the
    /// strings are stripped either way.
    pub enable_claude_magic_cache: bool,
    /// Place `prompt_cache_breakpoint` where a client embeds a magic cache
    /// string in an OpenAI Chat or Responses body.
    pub enable_openai_magic_cache: bool,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            fallback_mode: Default::default(),
            fallback_models: Vec::new(),
            referer: None,
            title: None,
            provider: None,
            model_providers: BTreeMap::new(),
            usage_accounting: true,
            normalize_service_tier: true,
            headers: BTreeMap::new(),
            enable_claude_magic_cache: false,
            enable_openai_magic_cache: false,
        }
    }
}

impl OpenRouterConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// The routing preferences for `model`: its own override, else the
    /// provider default. An entry that is not a JSON object is a
    /// configuration error rather than something to forward blindly.
    pub(super) fn routing(
        &self,
        model: Option<&str>,
    ) -> Result<Option<&Map<String, Value>>, ChannelError> {
        let value = model
            .and_then(|model| self.model_providers.get(model))
            .or(self.provider.as_ref());
        match value {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Object(map)) => Ok(Some(map)),
            Some(_) => Err(ChannelError::InvalidConfig(
                "provider routing preferences must be a JSON object".into(),
            )),
        }
    }
}
