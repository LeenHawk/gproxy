//! Bounded multi-candidate generation over concrete single-result edges.
//! Progress lives only in memory: a started child without an actual result is
//! never replayed, because nothing reloads a group once its invocation ends.

mod edges;
mod invoke;
pub(super) mod output;
mod prepare;
use super::{Endpoint, GenerationIdentity, GenerationProgress};
use crate::{
    codec::CodecLimits,
    transform::{
        TransformError, TransformErrorKind,
        identity::{IdNamespace, IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
};
pub use prepare::{
    ChatViaClaudeFanout, ChatViaResponsesFanout, FanoutTarget, GeminiViaClaudeFanout,
    GeminiViaResponsesFanout,
};

/// Host-supplied invocation namespace. Child namespaces are derived from it.
#[derive(Debug, Clone)]
pub struct FanoutOptions {
    pub namespace: IdNamespace,
    pub max_children: usize,
    pub response_policy: TargetIdPolicy,
}

/// Caller-owned evidence survives cancellation and failed post-send state writes,
/// so a caller can inspect which children were sent and what they returned. It
/// is never persisted and cannot restart a group: a prepared fanout sends at
/// most once, so a started POST is never sent twice.
#[derive(Debug)]
pub struct FanoutProgress<N> {
    children: Vec<GenerationProgress<N>>,
    group: GenerationProgress<()>,
}

impl<N> Default for FanoutProgress<N> {
    fn default() -> Self {
        Self {
            children: vec![],
            group: Default::default(),
        }
    }
}

impl<N> FanoutProgress<N> {
    pub fn children(&self) -> &[GenerationProgress<N>] {
        &self.children
    }
}

#[derive(Debug)]
struct Fanout<A> {
    children: Vec<A>,
    endpoint: Endpoint,
    group_id: String,
    options: FanoutOptions,
    /// Set once the first child may have been sent. Nothing persists a group,
    /// so this flag is what keeps a second `invoke` from repeating its POSTs.
    started: bool,
}

fn conflict(message: impl Into<String>) -> TransformError {
    TransformError::new(TransformErrorKind::Conflict, "generation.fanout", message)
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.fanout",
        "fanout budget exceeded",
    )
}

fn codec_error(e: crate::codec::CodecError) -> TransformError {
    TransformError::new(
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else {
            TransformErrorKind::InvalidResult
        },
        "generation.fanout",
        e.to_string(),
    )
}

pub(super) fn group_id(options: &FanoutOptions) -> Result<String, TransformError> {
    IdentityFlow::new(options.namespace)
        .resolve_or_allocate(
            IdentityRole::Response,
            SourceIdentity::new(options.response_policy.dialect, None, 0),
            &options.response_policy,
        )
        .map(|v| v.emitted_id)
        .map_err(|e| conflict(e.to_string()))
}

impl FanoutOptions {
    pub(in crate::adapt::generate) fn child_identity(
        &self,
        index: i64,
        upstream: crate::Dialect,
    ) -> GenerationIdentity {
        let namespace = |lane: u8| {
            let mut hash = blake3::Hasher::new_derive_key("gproxy.fanout.child.v1");
            hash.update(&self.namespace.bytes());
            hash.update(&index.to_le_bytes());
            hash.update(&[lane]);
            let mut bytes = [0; 16];
            bytes.copy_from_slice(&hash.finalize().as_bytes()[..16]);
            IdNamespace(bytes)
        };
        GenerationIdentity {
            request: IdentityFlow::new(namespace(0)),
            response: IdentityFlow::new(namespace(1)),
            request_policy: super::transport::generation_policy(upstream),
            response_policy: self.response_policy.clone(),
        }
    }
}

fn encode<T: serde::Serialize>(v: &T, limits: CodecLimits) -> Result<Vec<u8>, TransformError> {
    crate::codec::encode_json(v, limits)
        .map(|b| b.to_vec())
        .map_err(codec_error)
}
