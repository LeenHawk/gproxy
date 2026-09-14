use crate::{
    Dialect, HttpBody, WireRequest, WireResponse,
    capability::Upstream,
    codec::{self, CodecLimits},
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        identity::{IdNamespace, IdentityFlow, IdentityRole, KnownIdPrefix, TargetIdPolicy},
    },
    wire::DeclaredFields,
};
use serde::{Serialize, de::DeserializeOwned};

/// Host-selected operation path/query and headers. Authentication stays in Upstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub path: String,
    pub query: Option<String>,
    pub headers: http::HeaderMap,
}
impl Endpoint {
    pub fn new(path: impl Into<String>) -> Result<Self, TransformError> {
        let value = Self {
            path: path.into(),
            query: None,
            headers: Default::default(),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), TransformError> {
        if !self.path.starts_with('/')
            || self.path.starts_with("//")
            || self.path.contains(['?', '#', '\\', '\r', '\n'])
        {
            return Err(TransformError::shape(
                "endpoint.path",
                "expected an origin-relative path with separate query",
            ));
        }
        Ok(())
    }
}
/// Distinct invocation namespaces prevent request-history identities from colliding
/// with response output positions. The host reuses these namespaces on recovery.
#[derive(Debug, Clone)]
pub struct GenerationIdentity {
    pub request: IdentityFlow,
    pub response: IdentityFlow,
    pub request_policy: TargetIdPolicy,
    pub response_policy: TargetIdPolicy,
}
impl GenerationIdentity {
    pub fn new(
        request: IdNamespace,
        response: IdNamespace,
        client: Dialect,
        upstream: Dialect,
    ) -> Result<Self, TransformError> {
        if request == response {
            return Err(TransformError::shape(
                "identity.namespace",
                "request and response namespaces must differ",
            ));
        }
        Ok(Self {
            request: IdentityFlow::new(request),
            response: IdentityFlow::new(response),
            request_policy: generation_policy(upstream),
            response_policy: generation_policy(client),
        })
    }
    pub(super) fn validate(
        &self,
        client: Dialect,
        upstream: Dialect,
    ) -> Result<(), TransformError> {
        if self.request.namespace() == self.response.namespace()
            || self.request_policy.dialect != upstream
            || self.response_policy.dialect != client
        {
            return Err(TransformError::shape(
                "identity",
                "generation edge policy or namespace mismatch",
            ));
        }
        Ok(())
    }
}
/// Kept by the caller if an await is cancelled or response conversion fails.
/// A started send is never implicitly retried. Native bytes are retained before decoding.
#[derive(Debug)]
pub struct GenerationProgress<N> {
    pub send_started: bool,
    pub(super) exposed_response: Option<bytes::Bytes>,
    pub(super) binding: Option<InvocationBinding>,
    pub(super) saved_identities: super::state::SavedIdentities,
    pub raw_response: Option<WireResponse<bytes::Bytes>>,
    pub native_response: Option<N>,
}
impl<N> Default for GenerationProgress<N> {
    fn default() -> Self {
        Self {
            exposed_response: None,
            binding: None,
            saved_identities: Default::default(),
            send_started: false,
            raw_response: None,
            native_response: None,
        }
    }
}
#[derive(Debug)]
pub enum GenerationOutcome<C> {
    Success {
        response: WireResponse<C>,
        report: Report,
    },
    Rejected(WireResponse<bytes::Bytes>),
}
pub(super) async fn send<
    U: Upstream,
    I: Serialize + DeclaredFields,
    N: DeserializeOwned + DeclaredFields + Clone,
>(
    upstream: &U,
    target: &U::Target,
    endpoint: Endpoint,
    body: I,
    limits: CodecLimits,
    progress: &mut GenerationProgress<N>,
) -> Result<bool, TransformError> {
    if progress.send_started
        || progress.raw_response.is_some()
        || progress.native_response.is_some()
    {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "generation.send",
            "progress already records a send; recover its result instead of replaying",
        ));
    }
    endpoint.validate()?;
    let host = upstream.limits();
    let bytes = codec::encode_json(&body.into_declared(), bound(limits, host.write_bytes))
        .map_err(|e| {
            TransformError::with_source(
                if e.kind() == codec::CodecErrorKind::Limit {
                    TransformErrorKind::Limit
                } else {
                    TransformErrorKind::InvalidInput
                },
                "request.body",
                e.to_string(),
                e,
            )
        })?;
    let mut headers = endpoint.headers;
    json_headers(&mut headers);
    progress.send_started = true;
    let response = upstream
        .send(
            target,
            WireRequest {
                method: http::Method::POST,
                path: endpoint.path,
                query: endpoint.query,
                headers,
                body: HttpBody::Bytes(bytes),
            },
        )
        .await?;
    let WireResponse {
        status,
        headers,
        body,
    } = response;
    let bytes = codec::read_http_body(body, bound(limits, host.read_bytes))
        .await
        .map_err(codec_error)?;
    progress.raw_response = Some(WireResponse {
        status,
        headers,
        body: bytes,
    });
    if !status.is_success() {
        return Ok(false);
    }
    let raw = progress.raw_response.as_ref().expect("just retained");
    let native: N =
        codec::decode_json(&raw.body, bound(limits, host.read_bytes)).map_err(codec_error)?;
    progress.native_response = Some(native.into_declared());
    Ok(true)
}
pub(super) fn finish<C: Serialize, N>(
    progress: &mut GenerationProgress<N>,
    converted: Converted<C>,
    mut report: Report,
    limits: CodecLimits,
) -> Result<GenerationOutcome<C>, TransformError> {
    let encoded = codec::encode_json(&converted.value, limits).map_err(codec_error)?;
    if progress
        .exposed_response
        .as_ref()
        .is_some_and(|previous| previous != &encoded)
    {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "generation.recovery",
            "client response changed after first exposure",
        ));
    }
    progress.exposed_response = Some(encoded);
    let raw = progress
        .raw_response
        .as_ref()
        .expect("successful send retained");
    let mut headers = raw.headers.clone();
    json_headers(&mut headers);
    report.diagnostics.extend(converted.report.diagnostics);
    Ok(GenerationOutcome::Success {
        response: WireResponse {
            status: raw.status,
            headers,
            body: converted.value,
        },
        report,
    })
}
pub(super) fn rejected<C, N>(progress: &GenerationProgress<N>) -> GenerationOutcome<C> {
    let raw = progress
        .raw_response
        .as_ref()
        .expect("rejected send retained");
    GenerationOutcome::Rejected(WireResponse {
        status: raw.status,
        headers: raw.headers.clone(),
        body: raw.body.clone(),
    })
}
fn json_headers(headers: &mut http::HeaderMap) {
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
}
fn bound(mut limits: CodecLimits, bytes: u64) -> CodecLimits {
    limits.max_body_bytes = limits.max_body_bytes.min(bytes);
    limits.max_buffer_bytes = limits.max_buffer_bytes.min(bytes);
    limits.max_value_bytes = limits.max_value_bytes.min(bytes);
    limits
}
fn codec_error(error: codec::CodecError) -> TransformError {
    let kind = match error.kind() {
        codec::CodecErrorKind::Limit => TransformErrorKind::Limit,
        codec::CodecErrorKind::Transport => TransformErrorKind::Host,
        _ => TransformErrorKind::InvalidResult,
    };
    TransformError::with_source(kind, "upstream.response", error.to_string(), error)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InvocationBinding {
    endpoint: Endpoint,
    selected_model: String,
    request_namespace: IdNamespace,
    response_namespace: IdNamespace,
    request_policy: TargetIdPolicy,
    response_policy: TargetIdPolicy,
    original: bytes::Bytes,
    prepared: bytes::Bytes,
    state_target: crate::transform::identity::IdentityTarget,
    conversation: String,
    expiry: std::time::SystemTime,
}
pub(super) fn bind<
    S: crate::capability::StateStore,
    C: Serialize + DeclaredFields + Clone,
    I: Serialize + DeclaredFields + Clone,
    N,
>(
    endpoint: (&Endpoint, &str),
    identities: &GenerationIdentity,
    requests: (&C, &I),
    state: &super::GenerationStateAccess<'_, S>,
    limits: CodecLimits,
    progress: &mut GenerationProgress<N>,
    recovery: bool,
) -> Result<(), TransformError> {
    let (endpoint, model) = endpoint;
    let (original, prepared) = requests;
    let value = InvocationBinding {
        endpoint: endpoint.clone(),
        selected_model: model.into(),
        request_namespace: identities.request.namespace(),
        response_namespace: identities.response.namespace(),
        request_policy: identities.request_policy.clone(),
        response_policy: identities.response_policy.clone(),
        original: codec::encode_json(&original.clone().into_declared(), limits)
            .map_err(codec_error)?,
        prepared: codec::encode_json(&prepared.clone().into_declared(), limits)
            .map_err(codec_error)?,
        state_target: state.target.clone(),
        conversation: state.conversation_key.clone(),
        expiry: state.expires_at,
    };
    if let Some(existing) = &progress.binding {
        if existing != &value {
            return Err(TransformError::new(
                TransformErrorKind::Conflict,
                "generation.recovery",
                "prepared request or scope binding changed",
            ));
        }
    } else if recovery {
        return Err(TransformError::new(
            TransformErrorKind::MissingState,
            "generation.recovery",
            "progress is not bound to this invocation",
        ));
    } else {
        progress.binding = Some(value);
    }
    Ok(())
}

pub(super) fn recover_native<N: DeserializeOwned + DeclaredFields + Clone>(
    progress: &mut GenerationProgress<N>,
    limits: CodecLimits,
) -> Result<N, TransformError> {
    let raw = progress.raw_response.as_ref().ok_or_else(|| {
        TransformError::new(
            TransformErrorKind::MissingState,
            "generation.recovery",
            "no raw upstream response retained",
        )
    })?;
    if !raw.status.is_success() {
        return Err(TransformError::invalid_result(
            "generation.recovery",
            "success response required",
        ));
    }
    let native: N = codec::decode_json(&raw.body, limits).map_err(codec_error)?;
    let native = native.into_declared();
    progress.native_response = Some(native.clone());
    Ok(native)
}

fn generation_policy(dialect: Dialect) -> TargetIdPolicy {
    let policy = TargetIdPolicy::new(dialect);
    if dialect == Dialect::Claude {
        policy
            .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
            .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
    } else {
        policy
    }
}
