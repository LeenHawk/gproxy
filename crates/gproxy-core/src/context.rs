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
    OpenCode,
    /// Compatibility headers not tied to a particular client.
    Generic,
}
#[derive(Clone, Debug)]
pub struct SessionIdentity {
    pub id: String,
    pub source: SessionSource,
    /// Diagnostic header/body path only, not part of canonical affinity scope.
    pub field: Option<String>,
    /// The `agent_sessions` row the upper layer created for this identity,
    /// when it is a long-lived agent session whose credential binding core
    /// manages as assignments. None for ordinary affinity-only sessions.
    pub agent_session_id: Option<String>,
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
#[derive(Clone)]
pub struct ExecutionTarget {
    pub provider: Arc<ProviderData>,
    /// None for operations such as listing models that have no selected model.
    pub upstream_model: Option<String>,
    /// Original model name used by request rewrite filters, before alias resolution.
    pub requested_model: Option<String>,
    pub credentials: Vec<Arc<CredentialData>>,
}
#[derive(Clone)]
pub struct RequestContext {
    pub request_id: String,
    pub attribution: UsageAttribution,
    pub snapshot: Arc<CoreData>,
    /// Opaque isolation scope supplied after upper-layer authentication/admission.
    /// Core uses this for credential affinity; it does not interpret user/key roles.
    pub scope: String,
    pub session: Option<SessionIdentity>,
    pub operation: OperationKey,
    pub target: ExecutionTarget,
    /// The owners this request spends for, decided by the host after
    /// admission. Core checks every enabled budget of these owners that
    /// covers the upstream model before the first attempt and settles the
    /// priced cost into their windows afterwards. Empty means no budget.
    pub budgets: Vec<crate::budget::BudgetOwner>,
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
    /// The agent session assignment this attempt runs under, when the
    /// request named an agent session: the active one, or the generation this
    /// request reserved and is preparing.
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

/// Observations delivered to the Observer funnel and to the caller's completion
/// future. No financial policy, subscription allocation or pre-admission
/// counters live here.
#[derive(Clone, Debug)]
pub struct UsageReport {
    pub request_id: String,
    pub downstream_usage: Option<NormalizedUsage>,
    /// Physical upstream usage is reported once per call ID. Missing counts
    /// remain unknown rather than measured zero.
    pub exchanges: Vec<ExchangeUsage>,
    /// The request's USD charge. None when no exchange was priced
    /// (no usage, or no matching price rule).
    pub cost: Option<crate::pricing::Cost>,
    pub state: UsageState,
}
#[derive(Clone, Debug)]
pub struct ExchangeUsage {
    pub capture_id: String,
    pub attempt_id: String,
    pub attempt_ordinal: u32,
    pub provider_id: String,
    pub credential_id: String,
    pub upstream_model: Option<String>,
    pub usage: NormalizedUsage,
    /// Set at settlement from the snapshot's price book. None means no rule
    /// matched; `usage.dimensions["unpriced"]` is then `"true"`.
    pub cost: Option<crate::pricing::Cost>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageState {
    /// Receiving response headers does not finish metering a stream.
    Collecting,
    Completed,
    Cancelled,
    Failed,
    /// The request's ObservationPolicy disabled usage: nothing was extracted
    /// and the Observer funnel was not called. Distinct from upstream absence.
    Skipped,
}

/// Historical caller facts supplied by the host; never inferred from opaque scope.
#[derive(Clone, Debug, Default)]
pub struct UsageAttribution {
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
}
