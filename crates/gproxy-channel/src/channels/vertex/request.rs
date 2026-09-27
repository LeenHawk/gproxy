//! The `BaseChannel` implementation: which dialects Vertex serves natively,
//! and how one request is built. Every operation uses the default HTTP flow.

use super::config::VertexConfig;
use super::endpoint::{self, model_from_path, validate_model};
use super::{ID, Vertex, auth};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, HeaderAllowlist, PrepareContext, ProviderView,
    forwardable,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderValue, header};
use serde_json::Value;

/// The `anthropic_version` Vertex's Anthropic publisher requires; it replaces
/// the `anthropic-version` header the direct API uses.
const VERTEX_ANTHROPIC_VERSION: &str = "vertex-2023-10-16";


impl BaseChannel for Vertex {
    fn claude_fallback(&self) -> Option<crate::channel::ClaudeFallback> {
        Some(crate::channel::ClaudeFallback {
            credit: true,
            recommended_model: "claude-opus-4-8",
        })
    }

    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Google Vertex AI",
            login_modes: Vec::new(),
            capabilities: ChannelCapabilities {
                refresh: true,
                ..ChannelCapabilities::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "project",
                    ConfigKeyKind::String,
                    "Google Cloud project id. Required unless the service-account key carries its own project_id.",
                ),
                ConfigKey::optional(
                    "location",
                    ConfigKeyKind::String,
                    "Vertex region, for example us-central1, europe-west4 or global. Decides both the origin host and the method path; defaults to us-central1.",
                ).with_placeholder(super::config::DEFAULT_LOCATION),
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Complete origin, replacing the regional https://{location}-aiplatform.googleapis.com. Provider column, not config JSON.",
                ).with_placeholder("https://{location}-aiplatform.googleapis.com"),
            ]
            .into_iter()
            .chain(crate::channel::CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Vertex resells each publisher's own wire. Google's models speak Gemini,
    /// Anthropic's speak Claude Messages through `rawPredict`, and the
    /// OpenAI-compatible Chat surface is offered alongside both. Which
    /// publisher a provider actually has access to is routing configuration,
    /// not something this channel can know.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::Gemini, Dialect::Claude, Dialect::OpenAiChat]
            }
            Operation::CountTokens => vec![Dialect::Gemini, Dialect::Claude],
            Operation::ListModels
            | Operation::GetModel
            | Operation::CreateEmbedding
            | Operation::BatchCreateEmbedding => vec![Dialect::Gemini],
            _ => Vec::new(),
        }
    }

    /// The URL names the model for most of this upstream's methods, so a
    /// request whose model lives in the body has to be read to be addressed.
    /// A streamed request body cannot be read here, and the error says so
    /// rather than sending an unaddressed request.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = VertexConfig::from_view(ctx.provider)?;
        let place = endpoint::resolve(&config, ctx.provider, ctx.credential)?;
        let token = auth::access_token(ctx.credential.secret)?;
        let model = model_for(&ctx.request, ctx.operation)?;
        let uri = endpoint::method_url(
            &place,
            ctx.endpoint_override,
            ctx.operation,
            ctx.request.query.as_deref(),
            model.as_deref(),
        )?;
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        // Vertex's Anthropic publisher rejects the direct API's version header.
        if ctx.operation.dialect == Dialect::Claude {
            headers.remove("anthropic-version");
        }
        let body = body(ctx.request.body, ctx.operation)?;
        let mut builder = http::Request::builder()
            .method(ctx.request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
}

/// Where the upstream model comes from for each addressed method: Gemini puts
/// it in the path, Anthropic in the body, and the OpenAI-compatible surface's
/// URL names no model at all.
fn model_for(
    request: &WireRequest<HttpBody>,
    operation: OperationKey,
) -> Result<Option<String>, ChannelError> {
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, _) | (_, Dialect::OpenAiChat) => Ok(None),
        // Anthropic's token counter is addressed by a fixed model name.
        (Operation::CountTokens, Dialect::Claude) => Ok(None),
        (_, Dialect::Claude) => {
            let body = buffered(&request.body, operation)?;
            let value: Value = serde_json::from_slice(body)
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|model| !model.is_empty())
                .ok_or_else(|| {
                    ChannelError::InvalidConfig(
                        "a Claude request to Vertex must name its model in the body".into(),
                    )
                })?;
            validate_model(model)?;
            Ok(Some(model.to_owned()))
        }
        _ => Ok(model_from_path(&request.path).map(str::to_owned)),
    }
}

/// The publisher's own body, with the one difference Vertex insists on: an
/// Anthropic generation carries `anthropic_version` and must not carry
/// `model`, which the URL already names. Token counting keeps its `model`.
fn body(body: HttpBody, operation: OperationKey) -> Result<HttpBody, ChannelError> {
    if operation.dialect != Dialect::Claude {
        return Ok(body);
    }
    let bytes = buffered(&body, operation)?;
    let mut value: Value = serde_json::from_slice(bytes)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    let object = value.as_object_mut().ok_or_else(|| {
        ChannelError::InvalidConfig("a Claude request body must be a JSON object".into())
    })?;
    object
        .entry("anthropic_version")
        .or_insert_with(|| Value::String(VERTEX_ANTHROPIC_VERSION.into()));
    if operation.operation != Operation::CountTokens {
        object.remove("model");
    }
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    Ok(HttpBody::Bytes(Bytes::from(encoded)))
}

fn buffered(body: &HttpBody, operation: OperationKey) -> Result<&[u8], ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(_) => Err(ChannelError::InvalidConfig(format!(
            "{:?} on Vertex needs a buffered request body: the model and the \
             anthropic_version are read from it",
            operation.operation
        ))),
    }
}
