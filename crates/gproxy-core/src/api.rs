//! Reviewable function prototypes. These entry points deliberately return
//! NotImplemented until their execution paths are built; no fake success, I/O,
//! credential refresh or mutation is performed by the prototypes.

use crate::{
    AttemptContext, Core, CredentialData, CredentialVersion, RequestContext, RewriteRuleData,
    UsageReport,
};
use gproxy_channel::ChannelError;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    capability::{CapabilityFuture, UpstreamConnection},
};
use gproxy_store::entity::upstream::rewrite_rule;
use std::{collections::HashSet, num::NonZeroUsize, sync::Arc, time::Duration};

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("core function `{0}` is not implemented")]
    NotImplemented(&'static str),
    #[error("invalid execution target: {0}")]
    InvalidTarget(String),
    #[error("no usable credential in the supplied candidate set")]
    NoUsableCredential,
    #[error("request cancelled")]
    Cancelled,
    #[error("execution deadline exceeded")]
    DeadlineExceeded,
    #[error("rewrite failed: {0}")]
    Rewrite(String),
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error(transparent)]
    Cache(#[from] gproxy_cache::CacheError),
    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),
}

/// Must resolve when the response stream/socket finishes or is dropped, without
/// requiring a second read of its body. Native futures are Send; WASM follows
/// protocol's native future contract. The producer is not implemented yet.
pub type UsageCompletion = CapabilityFuture<'static, CoreResult<UsageReport>>;

/// Transport response and eventual observed usage are separate. Returning
/// response headers is not completion of a streaming invocation.
pub struct Execution<T> {
    pub response: T,
    pub usage: UsageCompletion,
}
pub type HttpExecution = Execution<WireResponse<HttpBody>>;
pub type WebSocketExecution = Execution<UpstreamConnection>;

#[derive(Clone, Copy, Debug)]
pub struct RewriteLimits {
    /// Input/output bound for a buffered body, complete event/message or selected
    /// header/query value. Not a request-wide allocation or transport time limit.
    pub max_unit_bytes: NonZeroUsize,
}

/// An attempt outcome is reported only after its actual completion. Response
/// headers/WS upgrade alone must not be counted as successful work.
pub enum AttemptOutcome {
    Succeeded {
        status: Option<http::StatusCode>,
    },
    Rejected {
        status: http::StatusCode,
        retry_after: Option<Duration>,
    },
    Failed(ChannelError),
    Cancelled,
    DeadlineExceeded,
}

/// Compile a single rule once during execution-data assembly. Contract includes
/// target/phase/name validation, regexes, paths and filters. No DB write occurs.
/// Query requires request phase; Header/Query reject paths and event filters.
pub fn compile_rewrite_rule(rule: Arc<rewrite_rule::Model>) -> CoreResult<RewriteRuleData> {
    let _ = rule;
    Err(CoreError::NotImplemented("compile_rewrite_rule"))
}

impl<C> Core<C> {
    /// Main HTTP entry point, including streaming bodies. The target is already
    /// routed/admitted. Retries may use only supplied credentials and must honor
    /// body replayability, side-effect semantics, cancellation and attempt budget.
    pub async fn send(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<HttpBody>,
    ) -> CoreResult<HttpExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented("send"))
    }

    /// Main WS entry point. Preserve both successful upgrades and rejected HTTP
    /// bodies. Established streams do not silently migrate to another account.
    pub async fn connect(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<()>,
    ) -> CoreResult<WebSocketExecution> {
        let _ = (context, request);
        Err(CoreError::NotImplemented("connect"))
    }

    /// Selection is confined to request.target.credentials, excluding attempted
    /// IDs and unusable candidates. Empty means NoUsableCredential, never a scan
    /// of the global provider pool or an upper-layer policy decision.
    /// RoundRobinAffinity advances rotation for an unbound session, not for a
    /// valid pin hit; unstable/request-scoped identities use ordinary rotation.
    pub async fn select_credential(
        &self,
        request: &RequestContext,
        excluded: &HashSet<String>,
    ) -> CoreResult<Arc<CredentialData>> {
        let _ = (request, excluded);
        Err(CoreError::NotImplemented("select_credential"))
    }

    /// Refresh this permitted attempt's credential under a shared lease. Persist
    /// a full replacement by Store CAS before publishing/returning a new version.
    /// A peer's newer durable version may be returned instead of refreshing again.
    pub async fn refresh_credential(
        &self,
        attempt: &AttemptContext,
    ) -> CoreResult<Arc<CredentialVersion>> {
        let _ = attempt;
        Err(CoreError::NotImplemented("refresh_credential"))
    }

    /// Apply request Body/Header/Query rules after protocol adaptation, before
    /// channel shaping. Only applicable Body rules may decode/buffer its payload.
    pub async fn rewrite_request(
        &self,
        attempt: &AttemptContext,
        request: WireRequest<HttpBody>,
        limits: RewriteLimits,
    ) -> CoreResult<WireRequest<HttpBody>> {
        let _ = (attempt, request, limits);
        Err(CoreError::NotImplemented("rewrite_request"))
    }

    /// Apply response Body/Header rules after channel handling and original usage
    /// observation, before adaptation to the caller. Query is never a response target.
    pub async fn rewrite_response(
        &self,
        attempt: &AttemptContext,
        response: WireResponse<HttpBody>,
        limits: RewriteLimits,
    ) -> CoreResult<WireResponse<HttpBody>> {
        let _ = (attempt, response, limits);
        Err(CoreError::NotImplemented("rewrite_response"))
    }

    /// Update credential health/affinity from a completed attempt. Upper-layer
    /// route affinity, billing and policy updates do not belong to this hook.
    pub async fn record_outcome(
        &self,
        attempt: &AttemptContext,
        outcome: AttemptOutcome,
        finished_at_ms: i64,
    ) -> CoreResult<()> {
        let _ = (attempt, outcome, finished_at_ms);
        Err(CoreError::NotImplemented("record_outcome"))
    }
}
