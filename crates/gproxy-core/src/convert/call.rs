//! One conversion call: everything a family driver needs, assembled by the
//! attempt loop once per attempt.

use crate::{AttemptUpstream, Core, ProtocolState, StateScope};
use gproxy_protocol::{
    Dialect, HttpBody, OperationKey, WireRequest, WireResponse,
    adapt::generate::GenerationStateAccess,
    codec::CodecLimits,
    connection::ByteStream,
    transform::{TransformError, identity::IdentityTarget},
};
use gproxy_seaorm::BatchConnectionTrait;
use std::time::{Duration, SystemTime};

/// How long continuation state written for a conversion stays valid.
pub(super) const STATE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub(super) const STATE_MAX_RECORDS: usize = 64;

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
    pub request: &'a WireRequest<HttpBody>,
    pub limits: CodecLimits,
    pub state_store: &'a ProtocolState<C>,
    pub state_scope: &'a StateScope,
    pub conversation_key: &'a str,
    pub provider_id: &'a str,
    pub now_ms: i64,
    /// The client asked for a stream but the upstream is invoked buffered;
    /// the driver synthesizes the client's native stream lifecycle.
    pub synthesize: bool,
}

impl<'a, C: BatchConnectionTrait + Send + Sync> Call<'a, C> {
    pub fn body(&self) -> &'a [u8] {
        match &self.request.body {
            HttpBody::Bytes(bytes) => bytes,
            HttpBody::Stream(_) => &[],
        }
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
            max_records: STATE_MAX_RECORDS,
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
