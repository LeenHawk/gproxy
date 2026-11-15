//! Counting the input tokens of a portable [`GenerateRequest`], like
//! generation: rendered for its model's dialect, then the parts a count
//! endpoint reads (messages, system, tools, reasoning settings) go into that
//! dialect's count request. Generation-only fields (`max_tokens`, `stream`,
//! sampling) are left out. Each dialect's mapping is in its own file; Chat
//! Completions has no count endpoint.
use crate::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "claude")]
mod claude;
#[cfg(feature = "gemini")]
mod gemini;
#[cfg(feature = "local-count")]
mod local;
#[cfg(feature = "openai")]
mod openai;

#[cfg(feature = "local-count")]
pub use local::estimate;

/// How many input tokens a request carries: as the upstream counted them, or
/// as estimated in process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokenCount {
    pub input_tokens: Option<u64>,
    /// Counted in process with a tokenizer that may not be the model's own.
    #[serde(default)]
    pub estimated: bool,
    /// The native count response under its vendor's `native` key.
    pub provider_metadata: ProviderMetadata,
}

impl<B: Backend> Client<B> {
    /// Count the input tokens `request` would send, with the upstream's count
    /// endpoint for its dialect. Chat Completions has none: that is
    /// [`ErrorKind::Unsupported`], for the caller to count another way.
    pub async fn count(&self, request: &GenerateRequest) -> Result<TokenCount> {
        match request.model.dialect {
            #[cfg(feature = "claude")]
            Dialect::Claude => self.count_tokens(&claude::params(request)?).await,
            #[cfg(feature = "openai")]
            Dialect::Openai => self.count_tokens(&openai::params(request)?).await,
            #[cfg(feature = "gemini")]
            Dialect::Gemini => self.count_tokens(&gemini::params(request)?).await,
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => {
                Err(Error::unsupported("Chat Completions has no count endpoint"))
            }
        }
    }
}

#[cfg(feature = "local-count")]
impl<B: Backend> Client<B> {
    /// [`Client::count`], or [`estimate`] where the upstream cannot count:
    /// a dialect without a count endpoint, or a count request that failed.
    pub async fn count_or_estimate(&self, request: &GenerateRequest) -> Result<TokenCount> {
        match self.count(request).await {
            Ok(count) if count.input_tokens.is_some() => Ok(count),
            _ => estimate(request),
        }
    }
}

/// The native count request for `request`'s dialect, for callers that send
/// it themselves (see [`Client::count_tokens`]).
pub fn params(request: &GenerateRequest) -> Result<operations::CountTokensParams> {
    match request.model.dialect {
        #[cfg(feature = "claude")]
        Dialect::Claude => claude::params(request),
        #[cfg(feature = "openai")]
        Dialect::Openai => openai::params(request),
        #[cfg(feature = "gemini")]
        Dialect::Gemini => gemini::params(request),
        #[cfg(feature = "chat")]
        Dialect::OpenaiChat => Err(Error::unsupported("Chat Completions has no count endpoint")),
    }
}
