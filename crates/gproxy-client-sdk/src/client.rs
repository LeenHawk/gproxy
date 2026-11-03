use crate::*;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use gproxy_protocol::Operation;
use gproxy_protocol::codec::{CodecLimits, read_http_body};
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
        let result = dialects::parse(&bytes, request);
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
        self.stream_with_asset_warnings(request, Vec::new()).await
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn generate_prepared(
        &self,
        prepared: PreparedGenerateRequest,
    ) -> Result<GenerateResult> {
        let mut result = self.generate(&prepared.request).await?;
        let mut warnings = prepared.warnings;
        warnings.extend(result.warnings);
        result.warnings = warnings;
        Ok(result)
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    pub async fn stream_prepared(
        &self,
        prepared: PreparedGenerateRequest,
    ) -> Result<GenerationStream> {
        self.stream_with_asset_warnings(&prepared.request, prepared.warnings)
            .await
    }
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini"
    ))]
    async fn stream_with_asset_warnings(
        &self,
        request: &GenerateRequest,
        mut asset_warnings: Vec<Warning>,
    ) -> Result<GenerationStream> {
        let (key, wire, warnings) = generation_wire(request, true)?;
        asset_warnings.extend(warnings);
        let response = self.send(key, Some(&request.model.id), wire).await?;
        stream::decode(
            response.body,
            request,
            self.limits,
            asset_warnings,
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
