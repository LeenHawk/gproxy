//! The gateway's local model catalog, read in the caller's chosen dialect.
//!
//! The dialect is the caller's choice, not a property of a model: the same
//! catalog is served in OpenAI, Claude and Gemini shapes, and the ids it
//! returns are the ones that dialect's requests accept at the same endpoint.
use super::empty_request;
use crate::*;
use gproxy_protocol::{Operation, WireRequest, connection::Bytes};
use http::Method;
use serde::{Deserialize, Serialize};

/// One catalog entry. `model` is ready to use in a request at the same endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModelInfo {
    pub model: Model,
    pub display_name: Option<String>,
    pub description: Option<String>,
    /// OpenAI `context_window`, Claude `max_input_tokens`, Gemini `inputTokenLimit`.
    pub context_window: Option<u64>,
    /// OpenAI `max_output_tokens`, Claude `max_tokens`, Gemini `outputTokenLimit`.
    pub max_output_tokens: Option<u64>,
    /// The native catalog entry under its vendor's `native` key.
    pub provider_metadata: ProviderMetadata,
}

impl<B: Backend> Client<B> {
    /// The permission-filtered catalog as `dialect` sees it.
    pub async fn list_models(&self, dialect: Dialect) -> Result<Vec<ModelInfo>> {
        let bytes = self.catalog(dialect, None).await?;
        match dialect {
            #[cfg(feature = "claude")]
            Dialect::Claude => {
                let body: gproxy_protocol::claude::models::ListModelsResponseBody = parse(&bytes)?;
                body.data.into_iter().map(claude).collect()
            }
            #[cfg(feature = "gemini")]
            Dialect::Gemini => {
                let body: gproxy_protocol::gemini::models::ListModelsResponseBody = parse(&bytes)?;
                body.models
                    .unwrap_or_default()
                    .into_iter()
                    .map(gemini)
                    .collect()
            }
            #[cfg(feature = "openai")]
            Dialect::Openai => openai_list(&bytes, dialect),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => openai_list(&bytes, dialect),
        }
    }

    /// One catalog entry, by the id and dialect a request would use.
    pub async fn get_model(&self, model: &Model) -> Result<ModelInfo> {
        let bytes = self.catalog(model.dialect, Some(&model.id)).await?;
        match model.dialect {
            #[cfg(feature = "claude")]
            Dialect::Claude => claude(parse(&bytes)?),
            #[cfg(feature = "gemini")]
            Dialect::Gemini => gemini(parse(&bytes)?),
            #[cfg(feature = "openai")]
            Dialect::Openai => openai(parse(&bytes)?, model.dialect),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => openai(parse(&bytes)?, model.dialect),
        }
    }

    async fn catalog(&self, dialect: Dialect, id: Option<&str>) -> Result<Bytes> {
        let (key, request) = request(dialect, id);
        let response = self.send(key, id, request).await?;
        let status = response.status.as_u16();
        let headers = response.headers;
        protocol::codec::read_http_body(response.body, self.limits)
            .await
            .map_err(|error| Error::from(error).with_head(status, &headers))
    }
}

fn request(dialect: Dialect, id: Option<&str>) -> (OperationKey, WireRequest<protocol::HttpBody>) {
    let operation = if id.is_some() {
        Operation::GetModel
    } else {
        Operation::ListModels
    };
    let base = match dialect {
        #[cfg(feature = "gemini")]
        Dialect::Gemini => "/v1beta/models",
        #[allow(unreachable_patterns)]
        _ => "/v1/models",
    };
    let path = match id {
        Some(id) => format!("{base}/{}", crate::client::path_segment(id)),
        None => base.to_owned(),
    };
    #[allow(unused_mut)]
    let mut request = empty_request(Method::GET, path);
    #[cfg(feature = "claude")]
    if dialect == Dialect::Claude {
        // The gateway tells Claude's catalog from OpenAI's by this header.
        request.headers.insert(
            "anthropic-version",
            http::HeaderValue::from_static("2023-06-01"),
        );
    }
    (
        OperationKey {
            operation,
            dialect: dialect.wire(),
        },
        request,
    )
}

fn parse<T: serde::de::DeserializeOwned>(bytes: &Bytes) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|error| Error::invalid(error).with_body(bytes))
}

fn metadata(vendor: Vendor, native: &impl Serialize) -> Result<ProviderMetadata> {
    let native = serde_json::to_value(native).map_err(Error::invalid)?;
    Ok([(
        vendor,
        [("native".to_owned(), native)].into_iter().collect(),
    )]
    .into())
}

#[cfg(any(feature = "claude", feature = "gemini"))]
fn limit(value: Option<i64>) -> Result<Option<u64>> {
    value.map(u64::try_from).transpose().map_err(Error::invalid)
}

#[cfg(feature = "claude")]
fn claude(native: gproxy_protocol::claude::models::ModelInfo) -> Result<ModelInfo> {
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Claude, &native)?,
        model: Model {
            id: native.id,
            dialect: Dialect::Claude,
        },
        display_name: Some(native.display_name),
        description: None,
        context_window: limit(native.max_input_tokens)?,
        max_output_tokens: limit(native.max_tokens)?,
    })
}

#[cfg(feature = "gemini")]
fn gemini(native: gproxy_protocol::gemini::models::Model) -> Result<ModelInfo> {
    // Gemini names the resource `models/{id}`; requests take the bare id.
    let id = native
        .name
        .strip_prefix("models/")
        .unwrap_or(&native.name)
        .to_owned();
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Gemini, &native)?,
        model: Model {
            id,
            dialect: Dialect::Gemini,
        },
        display_name: native.display_name,
        description: native.description,
        context_window: limit(native.input_token_limit)?,
        max_output_tokens: limit(native.output_token_limit)?,
    })
}

#[cfg(any(feature = "openai", feature = "chat"))]
fn openai_list(bytes: &Bytes, dialect: Dialect) -> Result<Vec<ModelInfo>> {
    // `models` is the Codex projection of the same catalog; `data` is the list.
    let body: gproxy_protocol::openai::models::ListModelsResponseBody = parse(bytes)?;
    body.data.into_iter().map(|m| openai(m, dialect)).collect()
}

#[cfg(any(feature = "openai", feature = "chat"))]
fn openai(native: gproxy_protocol::openai::models::Model, dialect: Dialect) -> Result<ModelInfo> {
    Ok(ModelInfo {
        provider_metadata: metadata(Vendor::Openai, &native)?,
        model: Model {
            id: native.id,
            dialect,
        },
        display_name: native.display_name,
        description: native.description,
        context_window: native.context_window,
        max_output_tokens: native.max_output_tokens,
    })
}
