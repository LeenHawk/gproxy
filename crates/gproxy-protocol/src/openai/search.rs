//! Standalone search compatibility wire.
//!
//! The `/v1/alpha/search` operation is proprietary/session-derived. The local
//! OpenAI snapshot documents web search only as a Responses tool and v2 has no
//! named public request/response structs, so this module asserts no fields.

use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SearchRequest {
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SearchResponse {
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opaque_search_models_preserve_every_field() {
        let request = json!({"query":"rust", "future_filter":{"x":1}});
        let parsed: SearchRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);

        let response = json!({"results":[{"url":"https://example.com"}], "future":true});
        let parsed: SearchResponse = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), response);
    }
}
