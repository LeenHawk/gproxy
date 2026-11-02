#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
use super::Params;
use crate::*;
#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
use gproxy_protocol::Operation;
#[cfg(feature = "openai")]
use gproxy_protocol::openai;
#[cfg(feature = "claude")]
use http::HeaderValue;
#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
use http::Method;
use serde::{Deserialize, Serialize};
#[cfg(feature = "count-tokens")]
#[derive(Debug, Clone, PartialEq)]
pub enum CountTokensParams {
    #[cfg(feature = "claude")]
    Claude(Params<gproxy_protocol::claude::count_tokens::CountTokensRequestBody>),
    #[cfg(feature = "openai")]
    Openai(Params<openai::count_tokens::CountTokensRequestBody>),
    #[cfg(feature = "gemini")]
    Gemini {
        model: String,
        params: Box<Params<gproxy_protocol::gemini::count_tokens::CountTokensRequestBody>>,
    },
}
#[cfg(feature = "count-tokens")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokenCount {
    pub input_tokens: Option<u64>,
    pub provider_metadata: ProviderMetadata,
}
#[cfg(all(
    feature = "count-tokens",
    any(feature = "claude", feature = "openai", feature = "gemini")
))]
impl<B: Backend> Client<B> {
    pub async fn count_tokens(&self, request: &CountTokensParams) -> Result<TokenCount> {
        let (vendor, count, native) = match request {
            #[cfg(feature = "claude")]
            CountTokensParams::Claude(p) => {
                let mut wire = wire_json(
                    Method::POST,
                    "/v1/messages/count_tokens".into(),
                    &native_params!(p),
                )?;
                wire.headers
                    .insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                let body: protocol::claude::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::Claude,
                        },
                        Some(&p.native.model),
                        wire,
                    )
                    .await?;
                (
                    Vendor::Claude,
                    Some(u64::try_from(body.input_tokens).map_err(Error::invalid)?),
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
            #[cfg(feature = "openai")]
            CountTokensParams::Openai(p) => {
                let model = p
                    .native
                    .model
                    .as_ref()
                    .and_then(Option::as_ref)
                    .map(String::as_str);
                let wire = wire_json(
                    Method::POST,
                    "/v1/responses/input_tokens".into(),
                    &native_params!(p),
                )?;
                let body: protocol::openai::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::OpenAi,
                        },
                        model,
                        wire,
                    )
                    .await?;
                (
                    Vendor::Openai,
                    Some(u64::try_from(body.input_tokens).map_err(Error::invalid)?),
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
            #[cfg(feature = "gemini")]
            CountTokensParams::Gemini { model, params: p } => {
                let wire = wire_json(
                    Method::POST,
                    format!("/v1beta/models/{}:countTokens", path_segment(model)),
                    &native_params!(p),
                )?;
                let body: protocol::gemini::count_tokens::CountTokensResponseBody = self
                    .json(
                        OperationKey {
                            operation: Operation::CountTokens,
                            dialect: protocol::Dialect::Gemini,
                        },
                        Some(model),
                        wire,
                    )
                    .await?;
                (
                    Vendor::Gemini,
                    body.total_tokens
                        .map(u64::try_from)
                        .transpose()
                        .map_err(Error::invalid)?,
                    serde_json::to_value(body).map_err(Error::invalid)?,
                )
            }
        };
        Ok(TokenCount {
            input_tokens: count,
            provider_metadata: [(vendor, [("native".into(), native)].into_iter().collect())].into(),
        })
    }
}
