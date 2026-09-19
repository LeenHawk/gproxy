//! Vendor CLI services without an OperationKey: profile, thread usage, bootstrap,
//! OAuth files/skills and remote-control endpoints. No gateway routing here.
//!
//! A service call sees two parties: the assigned upstream account
//! (`CredentialContext`) and the gateway caller (`ServiceCaller`). The caller
//! is the host's view of who is asking — an opaque identity, the usage the
//! host attributed to it and the upstream resources it created through the
//! gateway — so a channel can answer identity, usage and resource routes
//! without exposing the shared account behind the credential. `ServiceView`
//! says which picture the caller asked for; every declared route carries a
//! `ServiceClass` naming what the vendor endpoint returns, so the one table a
//! reviewer inspects shows how each route is handled under each view.

use gproxy_protocol::{HttpBody, WireRequest, WireResponse, capability::UpstreamConnection};
use http::Method;
use serde_json::Value;

use super::{ChannelError, CredentialContext, OperationFuture};
use gproxy_client::ClientBounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceTransport {
    Http,
    WebSocket,
}

/// Who the host says is calling. Core validates which views a role may use;
/// channels only shape answers (an admin's roles line reads "admin").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerRole {
    Admin,
    Member,
}

/// Which picture of the account a service call renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceView {
    /// The caller's own picture: the gateway's accounting for this caller and
    /// the resources this caller created. Never reflects any credential's
    /// real state. The only view a member may use.
    Caller,
    /// The provider's credential pool as one synthesized account: usage
    /// merged across credentials, resources across the provider's bindings,
    /// a provider-level identity. Admin only.
    Pool,
    /// One named credential, forwarded raw with its own authentication. Sees
    /// the real account. Admin only.
    Credential(String),
}

impl ServiceView {
    /// Caller and Pool are synthesized from `ServiceCaller` facts; only
    /// Credential reaches the vendor for account-bound routes.
    pub fn is_synthesized(&self) -> bool {
        !matches!(self, Self::Credential(_))
    }
}

/// How the caller may touch an upstream resource family through a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceAccess {
    /// Lists the account's resources; synthesized views list bindings.
    List,
    /// Reads or acts on one resource named in the path; gated by a binding
    /// and forwarded with the credential the binding names.
    Item,
    /// Creates a resource; the returned id is bound to the caller with the
    /// credential that created it.
    Create,
    /// Deletes a resource named in the path; the binding goes with it.
    Delete,
}

/// What a vendor route returns, from the shared account's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceClass {
    /// Caller-neutral catalog data (models, featured plugins, public keys):
    /// the same answer for any caller, forwarded under every view.
    Catalog,
    /// The account's identity (profile, whoami, roles, bootstrap identity):
    /// synthesized from `ServiceCaller::identity` unless the view is
    /// Credential.
    Identity,
    /// The account's usage windows and credits: synthesized from
    /// `ServiceCaller::usage` unless the view is Credential.
    Usage,
    /// Account-wide resources of one kind (tasks, files, plugins, skills):
    /// bindings decide what a synthesized view sees and may touch.
    Resource {
        kind: &'static str,
        access: ResourceAccess,
    },
    /// Account or organization settings, messages and configuration:
    /// neutral defaults (overridable from provider config) unless Credential.
    Settings,
    /// Billing, admin and device-binding operations on the account: 403
    /// unless the view is Credential.
    Restricted,
    /// Telemetry sinks: local empty success unless Credential, because the
    /// payload carries account context.
    Telemetry,
    /// Refused under every view (key minting on the shared account).
    Refused,
    /// A path under the vendor prefix the table does not know: 404 unless
    /// Credential, since what cannot be classified must not leak.
    Prefix,
}

#[derive(Debug, Clone)]
pub struct ServiceRoute {
    pub method: Method,
    /// Vendor path template, e.g. /api/oauth/files/{file_id}/content;
    /// `{*rest}` as the last segment matches the remaining path.
    pub path_template: &'static str,
    pub transport: ServiceTransport,
    /// Whether the host may replay the call (on another credential, after a
    /// transport failure). False for single-attempt mutations such as
    /// uploads, installs, enrolments and credit consumption.
    pub idempotent: bool,
    pub class: ServiceClass,
}

impl ServiceRoute {
    /// The captured template parameters when `method` and `path` match.
    pub fn matches<'p>(
        &self,
        method: &Method,
        path: &'p str,
    ) -> Option<Vec<(&'static str, &'p str)>> {
        (self.method == *method)
            .then(|| match_template(self.path_template, path))
            .flatten()
    }
}

/// Match `path` against a template of literal segments, `{name}` segments
/// and an optional trailing `{*name}` that captures the rest (which must be
/// non-empty). Returns the captured parameters in template order.
pub fn match_template<'p>(
    template: &'static str,
    path: &'p str,
) -> Option<Vec<(&'static str, &'p str)>> {
    let mut params = Vec::new();
    let mut rest = path.strip_prefix('/')?;
    let mut segments = template.strip_prefix('/')?.split('/').peekable();
    while let Some(segment) = segments.next() {
        if let Some(name) = segment.strip_prefix("{*").and_then(|s| s.strip_suffix('}')) {
            if segments.peek().is_some() || rest.is_empty() {
                return None;
            }
            params.push((name, rest));
            return Some(params);
        }
        let (head, tail) = match rest.split_once('/') {
            Some((head, tail)) => (head, tail),
            None => (rest, ""),
        };
        if head.is_empty() {
            return None;
        }
        if let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            params.push((name, head));
        } else if segment != head {
            return None;
        }
        if rest.len() == head.len() {
            rest = "";
            if segments.peek().is_some() {
                return None;
            }
        } else {
            rest = tail;
        }
    }
    rest.is_empty().then_some(params)
}

/// The host's stable, opaque name for the caller (or, under the Pool view,
/// for the provider). Channels derive vendor-shaped ids from it; they never
/// see a user record.
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
/// No windows means the host allots none; channels then omit window fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallerUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: Option<String>,
    pub windows: Vec<CallerUsageWindow>,
}

/// An upstream resource created through the gateway. `kind` is the
/// channel's vocabulary (`codex:task`, `claude:file`, ...), `credential_id`
/// the credential that created it (item routes forward with it), `summary`
/// the public metadata the vendor returned at creation, enough to list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceBindingRecord {
    pub kind: String,
    pub upstream_id: String,
    pub credential_id: String,
    pub summary: Value,
}

/// The host's facts behind a synthesized view. Under `Caller` they are the
/// caller's own; under `Pool` the whole provider's. The channel cannot tell
/// which beyond `role()`; it shapes what it is given. Failures are
/// `ChannelError::Host`.
pub trait ServiceCaller: ClientBounds {
    fn role(&self) -> CallerRole;
    fn identity(&self) -> &CallerIdentity;
    fn usage<'a>(&'a self) -> OperationFuture<'a, CallerUsage>;
    fn find_binding<'a>(
        &'a self,
        kind: &'a str,
        upstream_id: &'a str,
    ) -> OperationFuture<'a, Option<ResourceBindingRecord>>;
    fn list_bindings<'a>(
        &'a self,
        kind: &'a str,
    ) -> OperationFuture<'a, Vec<ResourceBindingRecord>>;
    fn save_binding<'a>(&'a self, record: ResourceBindingRecord) -> OperationFuture<'a, ()>;
    fn delete_binding<'a>(&'a self, kind: &'a str, upstream_id: &'a str)
    -> OperationFuture<'a, ()>;
}

pub struct ServiceContext<'a, B = HttpBody> {
    /// The credential the host selected for this call (under
    /// `ServiceView::Credential` the named one).
    pub account: CredentialContext<'a>,
    /// Every usable credential of the target. Item routes forward with the
    /// credential a binding names, looked up here by id.
    pub accounts: &'a [CredentialContext<'a>],
    pub caller: &'a dyn ServiceCaller,
    pub view: ServiceView,
    pub request: WireRequest<B>,
}

impl<'a, B> ServiceContext<'a, B> {
    /// The credential a binding names, when the host still has it.
    pub fn account_for(&self, credential_id: &str) -> Option<CredentialContext<'a>> {
        self.accounts
            .iter()
            .copied()
            .find(|account| account.credential.id == credential_id)
    }
}

/// Route declarations are metadata for server integration, not an execution gate.
/// Implementations can forward, compose calls or produce local responses. Local
/// aggregation, gateway OAuth issuance and durable affinity need host services;
/// they are not implied by this upstream-service contract.
pub trait ChannelServices: Send + Sync {
    fn routes(&self) -> &[ServiceRoute] {
        &[]
    }

    fn call<'a>(
        &'a self,
        _context: ServiceContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async { Err(ChannelError::UnsupportedService) })
    }

    fn connect<'a>(
        &'a self,
        _context: ServiceContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        Box::pin(async { Err(ChannelError::UnsupportedService) })
    }
}

#[cfg(test)]
mod tests {
    use super::match_template;

    #[test]
    fn templates_capture_named_and_rest_segments() {
        assert_eq!(match_template("/api/hello", "/api/hello"), Some(vec![]));
        assert_eq!(match_template("/api/hello", "/api/hello/x"), None);
        assert_eq!(
            match_template("/a/{id}/b", "/a/42/b"),
            Some(vec![("id", "42")])
        );
        assert_eq!(match_template("/a/{id}/b", "/a//b"), None);
        assert_eq!(
            match_template("/api/{*rest}", "/api/oauth/profile"),
            Some(vec![("rest", "oauth/profile")])
        );
        assert_eq!(match_template("/api/{*rest}", "/api/"), None);
        assert_eq!(match_template("/api/{*rest}", "/api"), None);
        assert_eq!(match_template("/a/{x}", "/a/1/2"), None);
    }
}
