//! One conversion call: everything a family driver needs, assembled by the
//! engine once per attempt.

use crate::Host;
use gproxy_protocol::{
    Dialect, HttpBody, OperationKey, WireRequest, WireResponse,
    adapt::generate::{GenerationResources, GenerationStateAccess},
    codec::CodecLimits,
    connection::{ByteStream, HeaderMap},
    transform::{TransformError, identity::IdentityTarget},
};
use std::time::{Duration, SystemTime};

/// How long continuation state written for a conversion stays valid.
pub const STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// How many foreign media references one generation request may have
/// fetched. Their bytes are bounded separately by the body and read limits.
const MAX_GENERATION_REFERENCES: usize = 16;

/// The client's request as a conversion family reads it: the addressing parts
/// plus the already-buffered body.
///
/// Deliberately not a `&WireRequest<HttpBody>`. `HttpBody::Stream` holds a
/// `ByteStream`, which is `Send` but not `Sync`, so `WireRequest<HttpBody>` is
/// not `Sync` and a shared reference to one is not `Send`. `Call` is borrowed
/// across every conversion await, so such a reference would travel into every
/// engine future and cost the whole public API its `Send` bound — the bound a
/// host needs to spawn one task per request. These parts are all `Sync`, and
/// conversion only ever runs on a request the attempt loop already buffered.
pub struct ClientRequest<'a> {
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a HeaderMap,
    /// The buffered request body, empty when the client sent none.
    pub body: &'a [u8],
}

impl<'a> ClientRequest<'a> {
    /// Borrow the parts of a buffered wire request. A body still streaming is
    /// read as empty, as conversion never sees one: the attempt loop refuses
    /// to convert a request it could not buffer.
    pub fn new(request: &'a WireRequest<HttpBody>) -> Self {
        Self {
            path: &request.path,
            query: request.query.as_deref(),
            headers: &request.headers,
            body: match &request.body {
                HttpBody::Bytes(bytes) => bytes,
                HttpBody::Stream(_) => &[],
            },
        }
    }
}

/// What the engine knows about one model of the provider.
#[derive(Clone, Copy)]
pub struct ModelFacts<'a> {
    pub upstream_name: &'a str,
    pub metadata: &'a serde_json::Value,
}

pub struct Call<'a, H: Host> {
    pub upstream: &'a H::Upstream,
    /// The client's operation and dialect.
    pub client: OperationKey,
    /// The upstream's native dialect this call converts to.
    pub target: Dialect,
    /// Resolved upstream model, when the operation has one.
    pub model: Option<&'a str>,
    /// The client's request with its body already buffered.
    pub request: ClientRequest<'a>,
    pub limits: CodecLimits,
    pub state_store: &'a H::State,
    pub state_scope: &'a H::StateScope,
    /// The scope of state any later request of the same caller and provider
    /// finds without a conversation (a video job).
    pub job_scope: &'a H::StateScope,
    /// Resource access in this caller's scope, bound to the provider and
    /// credentials of the attempt.
    pub resources: &'a H::Resources,
    pub resource_scope: &'a H::ResourceScope,
    pub conversation_key: &'a str,
    pub provider_id: &'a str,
    pub provider_name: &'a str,
    pub base_url: Option<&'a str>,
    pub models: &'a [ModelFacts<'a>],
    /// Idempotency key of this attempt's publications.
    pub operation_id: &'a str,
    /// Whether the host serves published resources under a URL.
    pub publishes_urls: bool,
    pub now_ms: i64,
    /// The client asked for a stream but the upstream is invoked buffered;
    /// the driver synthesizes the client's native stream lifecycle.
    pub synthesize: bool,
    /// Collect a native upstream stream into a complete client response.
    pub collect: bool,
}

impl<'a, H: Host> Call<'a, H> {
    pub fn body(&self) -> &'a [u8] {
        self.request.body
    }

    pub fn now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(self.now_ms.max(0) as u64)
    }

    pub fn model(&self) -> Result<&'a str, TransformError> {
        self.model
            .ok_or_else(|| TransformError::missing_metadata("upstream_model"))
    }

    /// The output budget the provider's directory advertises for the model.
    pub(crate) fn max_output_tokens(&self) -> Option<i64> {
        self.models
            .iter()
            .find(|model| Some(model.upstream_name) == self.model)
            .and_then(|model| model.metadata.get("max_output_tokens"))
            .and_then(serde_json::Value::as_i64)
    }

    /// Fetches the media references of a generation request that the target
    /// dialect cannot read itself; see `reference_passes_through`.
    pub(crate) fn generation_resources(&self) -> GenerationResources<'a, H::Resources> {
        GenerationResources {
            access: self.resources,
            scope: self.resource_scope,
            limits: self.limits,
            max_references: MAX_GENERATION_REFERENCES,
            now: self.now(),
            target: self.target,
        }
    }

    /// Continuation state bound to this scope, upstream and model.
    pub(crate) fn generation_state(
        &self,
    ) -> Result<GenerationStateAccess<'a, H::State>, TransformError> {
        self.generation_state_for(self.target)
    }

    /// Same, for an identity dialect other than the routed target (the
    /// Responses WebSocket upstream carries Responses identities).
    pub(crate) fn generation_state_for(
        &self,
        dialect: Dialect,
    ) -> Result<GenerationStateAccess<'a, H::State>, TransformError> {
        let target = IdentityTarget::new(self.model()?, dialect)
            .and_then(|t| t.with_origin(self.provider_id))
            .map_err(|e| TransformError::shape("identity.target", e.to_string()))?;
        Ok(GenerationStateAccess {
            store: self.state_store,
            scope: self.state_scope,
            target,
            conversation_key: self.conversation_key.to_owned(),
            expires_at: self.now() + STATE_TTL,
            now: self.now(),
        })
    }
}

/// What a family driver hands back to the attempt loop.
pub enum Converted {
    /// Complete client-dialect response; every upstream exchange has ended.
    Success(WireResponse<HttpBody>),
    /// The upstream rejected the call; body retained for the caller.
    Rejected(WireResponse<HttpBody>),
    /// A client-dialect stream still being driven; ending it ends the request.
    Stream(WireResponse<ByteStream>),
}
