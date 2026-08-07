//! Per-request context flowing through the pipeline, plus the small value types
//! produced by individual steps.

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method};

use crate::app::snapshot::KeyIdentity;
use crate::protocol::OperationKey;
use crate::store::persistence::records::{Credential, Provider};

/// How the inbound request was addressed.
#[derive(Clone)]
pub enum RoutingMode {
    /// `/v1/...` — model name resolves to a route via alias/route tables.
    Aggregated,
    /// `/{name}/v1/...` as parsed at the HTTP boundary. The pipeline resolves
    /// this to a public namespace first, then falls back to legacy provider scope.
    Named { name: String },
    /// Public logical service namespace such as `/openai/v1/...`.
    Namespace { namespace: String },
    /// Legacy `/{provider}/v1/...` provider bypass. Kept for compatibility; new
    /// public integrations should use [`RoutingMode::Namespace`].
    Scoped { provider: String },
}

/// Per-request context. Filled progressively as steps run.
#[derive(Clone)]
pub struct RequestCtx {
    pub request_id: String,
    pub method: Method,
    /// Provider-relative path (`/v1/...`); scoped mode already stripped of the
    /// leading `/{provider}`.
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub mode: RoutingMode,
    // filled by steps:
    pub identity: Option<Arc<KeyIdentity>>,
    pub op: Option<OperationKey>,
    pub stream: bool,
    /// Body `"model"` captured by classify's single body parse (`None` = the
    /// body carries no model or was never parsed — GETs, websocket upgrades).
    /// Downstream steps read this instead of re-parsing the body.
    pub body_model: Option<String>,
    pub route_name: Option<String>,
    /// §17 pre-deducted quota pending (micro-dollars), set by `execute` after
    /// authz passes; settle refunds this exact amount. 0 = no pre-deduct.
    pub pending_micros: i64,
}

/// One (member + credential) attempt for failover.
#[derive(Clone)]
pub struct Candidate {
    pub provider: Arc<Provider>,
    pub credential: Arc<Credential>,
    pub upstream_model_id: String,
    /// Route member behind this attempt; `None` in scoped mode (no member —
    /// the member breaker is skipped).
    pub member_id: Option<i64>,
    /// Cache key for optional route-member affinity. Set only for routed
    /// requests whose route enables affinity.
    pub(crate) member_affinity_key: Option<Arc<str>>,
    /// Hard binding for stateful upstream resources (Codex search/turn state).
    pub(crate) credential_binding_key: Option<Arc<str>>,
}

impl Candidate {
    pub(crate) fn for_provider(
        provider: Arc<Provider>,
        credential: Arc<Credential>,
        upstream_model_id: String,
    ) -> Self {
        Self {
            provider,
            credential,
            upstream_model_id,
            member_id: None,
            member_affinity_key: None,
            credential_binding_key: None,
        }
    }
}

/// Output of [`classify`](crate::pipeline::classify::classify).
pub struct Classified {
    pub op: OperationKey,
    pub stream: bool,
    /// Body `"model"` from the same single classify-time body parse.
    pub body_model: Option<String>,
}
