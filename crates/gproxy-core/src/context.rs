//! Inputs already routed and admitted by the upper layer, plus execution results.
//! Raw HTTP/WS bodies and transport results retain existing protocol types.

use crate::{CoreData, CredentialData, CredentialVersion, ProviderData};
use gproxy_channel::channel::NormalizedUsage;
use gproxy_protocol::OperationKey;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU32, sync::Arc};
use tokio_util::sync::CancellationToken;
use web_time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    Gateway,
    CodexThread,
    CodexSession,
    ClaudeCode,
    WorkBuddy,
    GeminiCli,
    Antigravity,
    GrokBuild,
    ConversationFingerprint,
    RequestFallback,
}
#[derive(Clone, Debug)]
pub struct SessionIdentity {
    pub id: String,
    pub source: SessionSource,
    /// Diagnostic header/body path only, not part of canonical affinity scope.
    pub field: Option<String>,
}
impl SessionIdentity {
    pub fn is_stable(&self) -> bool {
        self.source != SessionSource::RequestFallback
    }
}

/// Fully resolved and admitted by the upper layer. No route name, model alias,
/// identity or policy lookup occurs here. Future execution must select/retry only
/// within credentials; an empty list means no usable credential, never the whole
/// provider pool. Every credential must belong to this provider.
pub struct ExecutionTarget {
    pub provider: Arc<ProviderData>,
    /// None for operations such as listing models that have no selected model.
    pub upstream_model: Option<String>,
    pub credentials: Vec<Arc<CredentialData>>,
}
pub struct RequestContext {
    pub request_id: String,
    pub snapshot: Arc<CoreData>,
    /// Opaque isolation scope supplied after upper-layer authentication/admission.
    /// Core uses this for credential affinity; it does not interpret user/key roles.
    pub scope: String,
    pub session: Option<SessionIdentity>,
    pub operation: OperationKey,
    pub target: ExecutionTarget,
    /// Final budget supplied by the upper layer after applying its constraints.
    pub max_attempts: NonZeroU32,
    pub started_at_ms: i64,
    pub deadline: Option<Instant>,
    pub cancellation: CancellationToken,
}

/// One invocation attempt may issue multiple physical calls. Its provider/model
/// come from request.target and credential must come from that target's permitted
/// set. It pins one version rather than observing later refreshes in place.
pub struct AttemptContext {
    pub attempt_id: String,
    pub request: Arc<RequestContext>,
    pub ordinal: u32,
    pub credential: Arc<CredentialData>,
    pub credential_version: Arc<CredentialVersion>,
    /// Upper-layer association only; core does not select or migrate assignments.
    pub agent_assignment: Option<AgentAssignmentRef>,
}
#[derive(Clone, Debug)]
pub struct AgentAssignmentRef {
    pub session_id: String,
    pub assignment_id: String,
    pub generation: i64,
}
/// Allocate for each real outbound exchange, not every stream chunk.
pub struct ExchangeContext {
    pub capture_id: String,
    pub attempt: Arc<AttemptContext>,
    pub operation: OperationKey,
    pub started_at_ms: i64,
}

/// Observations returned to upper-layer logging/pricing/settlement. No financial
/// policy, subscription allocation or pre-admission counters live here.
pub struct UsageReport {
    pub request_id: String,
    pub downstream_usage: Option<NormalizedUsage>,
    /// Physical upstream usage is reported once per capture ID, without allocating
    /// it repeatedly across downstream consumers. None remains unknown, not zero.
    pub exchanges: Vec<ExchangeUsage>,
    pub state: UsageState,
}
pub struct ExchangeUsage {
    pub capture_id: String,
    pub provider_id: String,
    pub credential_id: String,
    pub upstream_model: Option<String>,
    pub usage: NormalizedUsage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageState {
    /// Receiving response headers does not finish metering a stream.
    Collecting,
    Completed,
    Cancelled,
    Failed,
}
