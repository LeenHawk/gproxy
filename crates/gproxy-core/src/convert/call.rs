//! One conversion call: everything a family driver needs, assembled by the
//! attempt loop once per attempt.

use crate::{AttemptUpstream, Core, ProtocolState, StateScope};
use gproxy_protocol::{
    Dialect, HttpBody, OperationKey, WireRequest, WireResponse,
    adapt::generate::GenerationStateAccess,
    codec::CodecLimits,
    connection::{ByteStream, HeaderMap},
    transform::{TransformError, identity::IdentityTarget},
};
use gproxy_seaorm::BatchConnectionTrait;
use std::time::{Duration, SystemTime};

/// How long continuation state written for a conversion stays valid.
pub(super) const STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

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
pub(crate) struct ClientRequest<'a> {
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

pub(crate) struct Call<'a, C> {
    /// The engine, for families that need Store-backed capabilities beyond
    /// the attempt-bound upstream (resources).
    pub core: &'a Core<C>,
    pub upstream: &'a AttemptUpstream,
    /// The client's operation and dialect.
    pub client: OperationKey,
    /// The upstream's native dialect this call converts to.
    pub target: Dialect,
    /// Resolved upstream model, when the operation has one.
    pub model: Option<&'a str>,
    /// The client's request with its body already buffered.
    pub request: ClientRequest<'a>,
    pub limits: CodecLimits,
    pub state_store: &'a ProtocolState<C>,
    pub state_scope: &'a StateScope,
    pub conversation_key: &'a str,
    pub provider_id: &'a str,
    pub now_ms: i64,
    /// The client asked for a stream but the upstream is invoked buffered;
    /// the driver synthesizes the client's native stream lifecycle.
    pub synthesize: bool,
    /// Collect a native upstream stream into a complete client response.
    pub collect: bool,
}

impl<'a, C: BatchConnectionTrait + Send + Sync> Call<'a, C> {
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

    /// Continuation state bound to this scope, upstream and model.
    pub fn generation_state(
        &self,
    ) -> Result<GenerationStateAccess<'a, ProtocolState<C>>, TransformError> {
        self.generation_state_for(self.target)
    }

    /// Same, for an identity dialect other than the routed target (the
    /// Responses WebSocket upstream carries Responses identities).
    pub fn generation_state_for(
        &self,
        dialect: Dialect,
    ) -> Result<GenerationStateAccess<'a, ProtocolState<C>>, TransformError> {
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
pub(crate) enum Converted {
    /// Complete client-dialect response; every upstream exchange has ended.
    Success(WireResponse<HttpBody>),
    /// The upstream rejected the call; body retained for the caller.
    Rejected(WireResponse<HttpBody>),
    /// A client-dialect stream still being driven; ending it ends the request.
    #[allow(dead_code)]
    Stream(WireResponse<ByteStream>),
}
