//! Per-request context flowing through the pipeline, plus the small value types
//! produced by individual steps.

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method};

use crate::app::snapshot::KeyIdentity;
use crate::health::config::BreakerConfig;
use crate::protocol::OperationKey;
use crate::store::persistence::records::{Credential, Provider};

/// How the inbound request was addressed.
pub enum RoutingMode {
    /// `/v1/...` — model name resolves to a route via alias/route tables.
    Aggregated,
    /// `/{provider}/v1/...` — bypass routing, hit the named provider directly.
    Scoped { provider: String },
}

/// Per-request context. Filled progressively as steps run.
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
    pub route_name: Option<String>,
    /// §17 pre-deducted quota pending (micro-dollars), set by `execute` after
    /// authz passes; settle refunds this exact amount. 0 = no pre-deduct.
    pub pending_micros: i64,
}

/// One (member + credential) attempt for failover.
pub struct Candidate {
    pub provider: Arc<Provider>,
    pub credential: Arc<Credential>,
    pub upstream_model_id: String,
    /// Route member behind this attempt; `None` in scoped mode (no member —
    /// the member breaker is skipped).
    pub member_id: Option<i64>,
    /// Effective breaker thresholds for this attempt's MEMBER breaker — the
    /// route's `circuit_breaker` override merged over the provider's (§3.2).
    /// Scoped mode (no route) carries the plain provider config.
    pub breaker_cfg: BreakerConfig,
}

/// Output of [`classify`](crate::pipeline::classify::classify).
pub struct Classified {
    pub op: OperationKey,
    pub stream: bool,
}
