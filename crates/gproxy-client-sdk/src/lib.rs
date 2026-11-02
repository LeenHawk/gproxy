//! Programs calling GPROXY use this crate. The embeddable engine is
//! `gproxy-sdk`; the outbound HTTP transport is `gproxy-client`.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

pub mod backend;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub mod dialects;
mod error;
pub mod ir;
mod resolution;
pub use resolution::ResolvedAsset;
#[cfg(feature = "embeddings")]
pub mod embeddings;
pub mod operations;
pub mod options;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod reasoning;
#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
mod sources;
pub mod stream;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod warnings;
pub use backend::Backend;
pub use error::{Error, ErrorKind, Result};
pub use gproxy_protocol as protocol;
pub use ir::*;
pub use stream::{Event, GenerationStream, aggregate};

#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use gproxy_protocol::Operation;
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse,
    codec::{CodecLimits, read_http_body},
};
use http::{HeaderMap, Method};
use serde::{Serialize, de::DeserializeOwned};

pub const DEFAULT_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 64 * 1024 * 1024,
    max_value_bytes: 64 * 1024 * 1024,
    max_body_bytes: 256 * 1024 * 1024,
    max_line_bytes: 64 * 1024 * 1024,
    max_part_bytes: 256 * 1024 * 1024,
    max_parts: 128,
};

pub struct Client<B> {
    pub backend: B,
    pub limits: CodecLimits,
}
impl<B> Client<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            limits: DEFAULT_LIMITS,
        }
    }
}
impl<B: Backend> Client<B> {
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn generate(&self, request: &GenerateRequest) -> Result<GenerateResult> {
        let (key, wire, warnings) = generation_wire(request, false)?;
        let response = self.send(key, Some(&request.model.id), wire).await?;
        let status = response.status.as_u16();
        let headers = response.headers;
        let bytes = read_http_body(response.body, self.limits)
            .await
            .map_err(|error| Error::from(error).with_head(status, &headers))?;
        let result = match request.model.dialect {
            #[cfg(feature = "claude")]
            Dialect::Claude => serde_json::from_slice(&bytes)
                .map_err(Error::invalid)
                .and_then(dialects::claude::parse),
            #[cfg(feature = "openai")]
            Dialect::Openai => serde_json::from_slice(&bytes)
                .map_err(Error::invalid)
                .and_then(dialects::openai::parse),
            #[cfg(feature = "gemini")]
            Dialect::Gemini => serde_json::from_slice(&bytes)
                .map_err(Error::invalid)
                .and_then(dialects::gemini::parse),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => serde_json::from_slice(&bytes)
                .map_err(Error::invalid)
                .and_then(|response| {
                    dialects::chat::parse(response, dialects::chat::output_format(request)?)
                }),
        };
        let mut result =
            result.map_err(|error| error.with_head(status, &headers).with_body(&bytes))?;
        let mut all = warnings;
        all.extend(result.warnings);
        result.warnings = all;
        Ok(result)
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn stream(&self, request: &GenerateRequest) -> Result<GenerationStream> {
        let (key, wire, warnings) = generation_wire(request, true)?;
        let response = self.send(key, Some(&request.model.id), wire).await?;
        stream::decode(
            response.body,
            request,
            self.limits,
            warnings,
            response.status.as_u16(),
            response.headers,
        )
    }
    #[allow(dead_code)] // Used by independently selectable operation families.
    pub(crate) async fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> Result<WireResponse<HttpBody>> {
        let response = self.backend.send(key, model, request).await?;
        if response.status.is_success() {
            return Ok(response);
        }
        let status = response.status.as_u16();
        let bytes = read_http_body(response.body, self.limits)
            .await
            .map_err(|error| Error::from(error).with_head(status, &response.headers))?;
        Err(Error::from_response(
            status,
            &response.headers,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }
    #[allow(dead_code)]
    pub(crate) async fn json<T: DeserializeOwned>(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> Result<T> {
        let response = self.send(key, model, request).await?;
        let status = response.status.as_u16();
        let headers = response.headers;
        let bytes = read_http_body(response.body, self.limits)
            .await
            .map_err(|error| Error::from(error).with_head(status, &headers))?;
        serde_json::from_slice(&bytes).map_err(|error| {
            Error::invalid(error)
                .with_head(status, &headers)
                .with_body(&bytes)
        })
    }
}
#[allow(dead_code)]
pub(crate) fn path_segment(value: &str) -> String {
    // Encode separators as well: a model/router alias is one path segment.
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}
#[allow(dead_code)]
pub(crate) fn wire_json(
    method: Method,
    path: String,
    body: &impl Serialize,
) -> Result<WireRequest<HttpBody>> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    Ok(WireRequest {
        method,
        path,
        query: None,
        headers,
        body: HttpBody::Bytes(serde_json::to_vec(body).map_err(Error::invalid)?.into()),
    })
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
fn generation_wire(
    request: &GenerateRequest,
    streaming: bool,
) -> Result<(OperationKey, WireRequest<HttpBody>, Vec<Warning>)> {
    let operation = if streaming {
        Operation::StreamGenerateContent
    } else {
        Operation::GenerateContent
    };
    let path = match request.model.dialect {
        #[cfg(feature = "claude")]
        Dialect::Claude => "/v1/messages".into(),
        #[cfg(feature = "openai")]
        Dialect::Openai => "/v1/responses".into(),
        #[cfg(feature = "chat")]
        Dialect::OpenaiChat => "/v1/chat/completions".into(),
        #[cfg(feature = "gemini")]
        Dialect::Gemini => format!(
            "/v1beta/models/{}:{}",
            path_segment(&request.model.id),
            if streaming {
                "streamGenerateContent"
            } else {
                "generateContent"
            }
        ),
    };
    fn encode<T: Serialize>(
        path: String,
        rendered: Rendered<T>,
    ) -> Result<(WireRequest<HttpBody>, Vec<Warning>)> {
        Ok((
            wire_json(Method::POST, path, &rendered.request)?,
            rendered.warnings,
        ))
    }
    let (mut wire, warnings) = match request.model.dialect {
        #[cfg(feature = "claude")]
        Dialect::Claude => encode(path, dialects::claude::render(request, streaming)?)?,
        #[cfg(feature = "openai")]
        Dialect::Openai => encode(path, dialects::openai::render(request, streaming)?)?,
        #[cfg(feature = "gemini")]
        Dialect::Gemini => encode(path, dialects::gemini::render(request)?)?,
        #[cfg(feature = "chat")]
        Dialect::OpenaiChat => encode(path, dialects::chat::render(request, streaming)?)?,
    };
    if request.model.dialect.wire() == gproxy_protocol::Dialect::Claude {
        wire.headers.insert(
            "anthropic-version",
            http::HeaderValue::from_static("2023-06-01"),
        );
    }
    if streaming {
        wire.headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/event-stream"),
        );
        if request.model.dialect.wire() == gproxy_protocol::Dialect::Gemini {
            wire.query = Some("alt=sse".into());
        }
    }
    Ok((
        OperationKey {
            operation,
            dialect: request.model.dialect.wire(),
        },
        wire,
        warnings,
    ))
}

/// Export the complete public IR graph using the workspace's ts-rs setup.
/// The caller chooses the output directory; normal builds write nothing.
#[cfg(feature = "ts")]
pub fn export_types(
    directory: impl Into<std::path::PathBuf>,
) -> std::result::Result<(), ts_rs::ExportError> {
    use ts_rs::TS;
    let config = ts_rs::Config::new()
        .with_out_dir(directory)
        .with_large_int("number");
    Conversation::export_all(&config)?;
    GenerateRequest::export_all(&config)?;
    GenerateResult::export_all(&config)?;
    Event::export_all(&config)?;
    #[cfg(feature = "embeddings")]
    {
        embeddings::EmbedRequest::export_all(&config)?;
        embeddings::EmbedResult::export_all(&config)?;
    }
    operations::Params::<ts_rs::Dummy>::export_all(&config)?;
    #[cfg(feature = "count-tokens")]
    operations::TokenCount::export_all(&config)?;
    #[cfg(feature = "audio")]
    operations::TranscriptionParams::export_all(&config)?;
    Ok(())
}
