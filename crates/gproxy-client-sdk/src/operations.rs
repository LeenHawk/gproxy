//! Native passthrough operation families, independent of the portable embed API.
use crate::{ProviderOptions, Vendor};
use serde::{Deserialize, Serialize};
/// Native typed parameters plus options for the selected target vendor.
/// No universal media/embedding format is imposed on the response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Params<T> {
    pub native: T,
    pub vendor: Vendor,
    #[serde(default)]
    pub provider_options: ProviderOptions,
}
impl<T> Params<T> {
    pub fn new(native: T, vendor: Vendor) -> Self {
        Self {
            native,
            vendor,
            provider_options: ProviderOptions::new(),
        }
    }
}
#[cfg(any(
    feature = "embeddings",
    feature = "rerank",
    feature = "audio",
    feature = "images",
    feature = "video",
    all(
        feature = "count-tokens",
        any(feature = "claude", feature = "openai", feature = "gemini")
    )
))]
macro_rules! native_params {
    ($params:expr) => {{
        let params = $params;
        let mut native = params.native.clone();
        let mut rest = params.provider_options.remainder(&params.vendor)?.clone();
        rest.extend(native.rest);
        native.rest = rest;
        native
    }};
}

#[cfg(feature = "count-tokens")]
mod count_tokens;
#[cfg(any(
    feature = "embeddings",
    feature = "rerank",
    feature = "images",
    feature = "video"
))]
mod json;
#[cfg(feature = "count-tokens")]
pub use count_tokens::CountTokensParams;
#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "video")]
mod video;
#[cfg(feature = "audio")]
pub use audio::TranscriptionParams;
#[cfg(any(feature = "models", feature = "video"))]
pub(crate) fn empty_request(
    method: http::Method,
    path: String,
) -> crate::protocol::WireRequest<crate::protocol::HttpBody> {
    use gproxy_protocol::{HttpBody, WireRequest, connection::Bytes};
    use http::HeaderMap;
    WireRequest {
        method,
        path,
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}
