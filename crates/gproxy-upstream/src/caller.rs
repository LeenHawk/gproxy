//! The gateway caller as an upstream implementation sees it: an opaque
//! identity, the usage the host attributed to it and the upstream resources
//! it created through the gateway.

use gproxy_client::ClientBounds;
use serde_json::Value;

use crate::UpstreamFuture;

/// Who the host says is calling. The host validates what a role may do;
/// implementations only shape answers (an admin's roles line reads "admin").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerRole {
    Admin,
    Member,
}

/// The host's stable, opaque name for the caller (or, under a pooled view,
/// for the provider). Implementations derive vendor-shaped ids from it; they
/// never see a user record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerIdentity {
    pub id: String,
    pub display_name: Option<String>,
}

/// One usage window the host tracks, in the host's own key vocabulary
/// (`primary`/`secondary`, `5h`/`7d`, a model family, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct CallerUsageWindow {
    pub key: String,
    pub used_percent: Option<f64>,
    pub period_start_ms: Option<i64>,
    pub reset_at_ms: Option<i64>,
}

/// What the host attributed to the caller (or merged for the pool): lifetime
/// tokens, settled cost as a decimal string, and the windows it meters.
/// No windows means the host allots none; implementations then omit window fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallerUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: Option<String>,
    pub windows: Vec<CallerUsageWindow>,
}

/// An upstream resource created through the gateway. `kind` is the
/// implementation's vocabulary (`codex:task`, `claude:file`, ...),
/// `credential_id` the credential that created it (item routes forward with
/// it), `summary` the public metadata the vendor returned at creation, enough
/// to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBindingRecord {
    pub kind: String,
    pub upstream_id: String,
    pub credential_id: String,
    pub summary: Value,
}

/// The host's facts about the caller. Under a pooled view they are the whole
/// provider's; the implementation cannot tell which beyond `role()` and
/// shapes what it is given. Failures are `UpstreamError::Host`.
pub trait ServiceCaller: ClientBounds {
    fn role(&self) -> CallerRole;
    fn identity(&self) -> &CallerIdentity;
    fn usage<'a>(&'a self) -> UpstreamFuture<'a, CallerUsage>;
    fn find_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> UpstreamFuture<'a, Option<ResourceBindingRecord>>;
    fn list_bindings<'a>(&'a self, kind: &'a str)
    -> UpstreamFuture<'a, Vec<ResourceBindingRecord>>;
    fn save_binding<'a>(&'a self, record: ResourceBindingRecord) -> UpstreamFuture<'a, ()>;
    fn delete_binding<'a>(&'a self, kind: &'a str, upstream_id: &'a str) -> UpstreamFuture<'a, ()>;
}
