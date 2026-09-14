//! Bounded, journaled multi-candidate generation over concrete single-result edges.
//! Every child POST is reserved durably. A started child without an actual result
//! requires reconciliation and is never automatically replayed.
mod edges;
mod invoke;
mod journal;
pub(super) mod output;
mod prepare;
use super::{Endpoint, GenerationIdentity, GenerationProgress};
use crate::{
    capability::Version,
    codec::CodecLimits,
    transform::{
        TransformError, TransformErrorKind,
        identity::{IdNamespace, IdentityFlow, IdentityRole, SourceIdentity},
    },
};
use journal::Journal;
pub use prepare::{
    ChatViaClaudeFanout, ChatViaResponsesFanout, FanoutTarget, GeminiViaClaudeFanout,
    GeminiViaResponsesFanout,
};

/// Host-supplied unique aggregate namespace, distinct from all child namespaces.
#[derive(Debug, Clone, Copy)]
pub struct FanoutOptions {
    pub namespace: IdNamespace,
    pub max_children: usize,
}
/// Caller-owned evidence survives cancellation and failed post-send state writes.
/// `resume` additionally reads the durable journal; no started POST is replayed.
#[derive(Debug)]
pub struct FanoutProgress<N> {
    journal: Option<Journal>,
    version: Option<Version>,
    children: Vec<GenerationProgress<N>>,
    group: GenerationProgress<()>,
}
impl<N> Default for FanoutProgress<N> {
    fn default() -> Self {
        Self {
            journal: None,
            version: None,
            children: vec![],
            group: Default::default(),
        }
    }
}
impl<N> FanoutProgress<N> {
    pub fn children(&self) -> &[GenerationProgress<N>] {
        &self.children
    }
    pub fn response_id(&self) -> Option<&str> {
        self.journal.as_ref().map(|j| j.binding.id.as_str())
    }
}
#[derive(Debug)]
struct Fanout<A> {
    children: Vec<A>,
    endpoint: Endpoint,
    group_id: String,
    original: Vec<u8>,
    options: FanoutOptions,
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
pub(super) fn group_id(
    options: FanoutOptions,
    ids: &[GenerationIdentity],
) -> Result<String, TransformError> {
    let Some(first) = ids.first() else {
        return Err(limit());
    };
    if ids.len() > options.max_children || ids.len() < 2 {
        return Err(limit());
    }
    let mut used = std::collections::HashSet::from([options.namespace]);
    for value in ids {
        value.validate(first.response_policy.dialect, first.request_policy.dialect)?;
        if value.response_policy != first.response_policy {
            return Err(TransformError::shape(
                "fanout.response_policy",
                "one aggregate requires one consistent client identity policy",
            ));
        }
        if !used.insert(value.request.namespace()) || !used.insert(value.response.namespace()) {
            return Err(TransformError::shape(
                "fanout.namespaces",
                "each request, response and aggregate requires a distinct namespace",
            ));
        }
    }
    IdentityFlow::new(options.namespace)
        .resolve_or_allocate(
            IdentityRole::Response,
            SourceIdentity::new(first.response_policy.dialect, None, 0),
            &first.response_policy,
        )
        .map(|v| v.emitted_id)
        .map_err(|e| conflict(e.to_string()))
}
fn encode<T: serde::Serialize>(v: &T, limits: CodecLimits) -> Result<Vec<u8>, TransformError> {
    crate::codec::encode_json(v, limits)
        .map(|b| b.to_vec())
        .map_err(codec_error)
}
