//! Turning a [`Caller`] into an [`Admitted`]: everything the engine needs to
//! run this request, decided once, here.
//!
//! Nothing downstream re-derives any of it. The sdk is handed a provider set,
//! a credential set, a budget owner chain, a scope and a session identity —
//! not a caller to interpret — which is what keeps "who may use this" from
//! being answered twice, differently, in two layers.
//!
//! ## The order, and why it is this one
//!
//! 1. **permissions** → the allowed provider set. Refuses first, because a
//!    caller who may reach nothing should cost nothing.
//! 2. **credential visibility** → the allowed credential set. The calling
//!    key's organization/team binding is the tenant boundary.
//! 3. **the budget chain** → who pays.
//! 4. **the scope** → whose traffic this counts as, for credential affinity.
//! 5. **the session identity** → which conversation this continues.
//! 6. **rate limits** → the only step that *consumes* something, and
//!    therefore the last one.
//!
//! Steps 1–5 are pure functions of the snapshot and the request. Step 6 moves
//! a shared counter, so it runs only once the request is known to be
//! otherwise admissible: a request rejected for permissions must not spend a
//! window a legitimate caller needs, or an unauthorized client could exhaust
//! somebody else's limit by sending requests it was never going to be allowed
//! to make.

pub mod attribution;
pub mod budgets;
pub mod credential;
pub mod permission;
pub mod rate_limit;
pub mod scope;
pub mod session;

pub use rate_limit::RateLease;
pub use session::{GATEWAY_SESSION_HEADER, strip_gateway_header};

use crate::{AppConfig, AppData, AppError, Caller};
use gproxy_cache::Cache;
use gproxy_core::{BudgetOwner, SessionIdentity, UsageAttribution};
use gproxy_protocol::OperationKey;
use gproxy_store::Store;
use http::HeaderMap;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

/// One request, as much of it as admission looks at.
///
/// `all_providers` and `all_credentials` are the instance's **live** sets as
/// the engine sees them — what `CoreData` holds after disabled, retired and
/// fully blocked rows are gone. Admission narrows them; it never adds to them,
/// so a credential the engine cannot use is not resurrected by a caller who
/// happens to own it.
pub struct AdmissionRequest<'a> {
    pub caller: &'a Caller,
    pub operation: OperationKey,
    /// The model the client asked for, `None` for operations that name none.
    pub model: Option<&'a str>,
    pub headers: &'a HeaderMap,
    /// The decoded request body, when there is a JSON one. Read only by the
    /// session ladder.
    pub body: Option<&'a Value>,
    pub request_id: &'a str,
    /// An `agent_sessions` row the host has already resolved for this
    /// request, when this is a long-lived agent session whose credential
    /// binding core manages as assignments rather than as affinity.
    pub agent_session_id: Option<&'a str>,
    pub all_providers: &'a BTreeSet<String>,
    pub all_credentials: &'a BTreeSet<String>,
    /// The clock this request is decided against, which fixes the rate-limit
    /// window. Taken as a parameter rather than read here so that every stage
    /// of one request agrees on the time.
    pub now_ms: i64,
}

impl<'a> AdmissionRequest<'a> {
    /// The required parts, with no model, no body, no agent session and the
    /// current clock.
    pub fn new(
        caller: &'a Caller,
        operation: OperationKey,
        headers: &'a HeaderMap,
        request_id: &'a str,
        all_providers: &'a BTreeSet<String>,
        all_credentials: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            caller,
            operation,
            model: None,
            headers,
            body: None,
            request_id,
            agent_session_id: None,
            all_providers,
            all_credentials,
            now_ms: crate::now_ms(),
        }
    }

    pub fn model(mut self, model: Option<&'a str>) -> Self {
        self.model = model;
        self
    }

    pub fn body(mut self, body: Option<&'a Value>) -> Self {
        self.body = body;
        self
    }

    pub fn agent_session_id(mut self, agent_session_id: Option<&'a str>) -> Self {
        self.agent_session_id = agent_session_id;
        self
    }

    pub fn at(mut self, now_ms: i64) -> Self {
        self.now_ms = now_ms;
        self
    }
}

/// What the engine is given. Every field is a decision, not an input to one.
///
/// `providers` and `credentials` are both narrowing sets and both are applied:
/// a provider in the set whose every credential is invisible contributes no
/// target, which is the failure the resolver reports rather than a 403.
#[derive(Debug)]
pub struct Admitted {
    /// The affinity scope, `user:{id}` or `grant:{id}`.
    pub scope: String,
    pub attribution: UsageAttribution,
    pub budgets: Vec<BudgetOwner>,
    pub providers: BTreeSet<String>,
    pub credentials: BTreeSet<String>,
    pub session: Option<SessionIdentity>,
    /// The rate-limit charges this request is holding. Keep them alive for
    /// the whole request: dropping them returns the charges.
    pub rate_limit_leases: Vec<RateLease>,
}

impl Admitted {
    /// The request ran.
    ///
    /// Window counters stand — a request was made — while concurrency permits
    /// are still returned when the leases drop, because they measure requests
    /// in flight. Idempotent.
    pub fn finish(&mut self) {
        for lease in &mut self.rate_limit_leases {
            lease.finish();
        }
    }

    /// Give every outstanding charge back now, awaiting the cache rather than
    /// leaving it to the drop path.
    ///
    /// A host that can await at the end of a request should: the drop path
    /// spawns, which needs a runtime and completes at an unspecified later
    /// point, whereas this has finished by the time it returns.
    pub async fn release(&mut self) {
        for lease in &mut self.rate_limit_leases {
            lease.release().await;
        }
    }
}

/// Admission against one snapshot, one cache and one configuration.
///
/// Borrowed rather than owned, for the same reason [`Authenticator`] is: the
/// request already holds the `Arc<AppData>` it authenticated under, and the
/// point of a snapshot is that one value answers every question in one
/// request.
///
/// [`Authenticator`]: crate::Authenticator
pub struct Admission<'a, C> {
    store: &'a Store<C>,
    snapshot: &'a AppData,
    cache: &'a Arc<dyn Cache>,
    config: &'a AppConfig,
}

impl<'a, C> Admission<'a, C> {
    pub fn new(
        store: &'a Store<C>,
        snapshot: &'a AppData,
        cache: &'a Arc<dyn Cache>,
        config: &'a AppConfig,
    ) -> Self {
        Self {
            store,
            snapshot,
            cache,
            config,
        }
    }

    /// The database this admission reads. Nothing in the steps below needs it
    /// yet — every decision is answered by the snapshot and the cache, which
    /// is the property that keeps admission off the hot path — but the
    /// operations built on top of an `Admission` do.
    pub fn store(&self) -> &'a Store<C> {
        self.store
    }

    pub fn snapshot(&self) -> &'a AppData {
        self.snapshot
    }

    pub fn cache(&self) -> &'a Arc<dyn Cache> {
        self.cache
    }

    pub fn config(&self) -> &'a AppConfig {
        self.config
    }

    /// Run the six steps in order and produce what the engine executes with.
    ///
    /// Returns `Forbidden` when no provider is permitted or an OAuth client
    /// asked for an operation outside its baseline, and `RateLimited` when a
    /// configured limit refused the request or could not be evaluated.
    pub async fn admit(&self, request: AdmissionRequest<'_>) -> Result<Admitted, AppError> {
        let AdmissionRequest {
            caller,
            operation,
            model,
            headers,
            body,
            request_id,
            agent_session_id,
            all_providers,
            all_credentials,
            now_ms,
        } = request;

        let providers = permission::allowed_providers(
            self.snapshot,
            caller,
            model,
            operation.operation,
            all_providers,
            &self.config.oauth.cli_client_ids,
        )?;

        let credentials = credential::visible_credentials(self.snapshot, caller, all_credentials);
        if credentials.is_empty() {
            // Not an error: "nothing exists to use" is resolution's report,
            // not admission's. Worth a line, because it is what an operator
            // sees as an unexplained `NoTarget` in the console.
            tracing::debug!(
                request_id,
                user_id = %caller.user_id,
                api_key_id = caller.api_key_id.as_deref().unwrap_or("-"),
                live_credentials = all_credentials.len(),
                "no credential is visible to this caller"
            );
        }

        let budgets = budgets::chain(caller);
        let scope = scope::render(caller);
        let session = session::extract(headers, body, operation, request_id, agent_session_id);

        // Last: the only step that consumes a shared allowance.
        let rate_limit_leases =
            rate_limit::apply(self.snapshot, self.cache, caller, model, now_ms).await?;

        Ok(Admitted {
            scope,
            attribution: attribution::attribution(caller, model),
            budgets,
            providers,
            credentials,
            session,
            rate_limit_leases,
        })
    }
}

#[cfg(test)]
pub(crate) mod support {
    use crate::{Caller, CallerKind};

    /// A plain API-key caller holding key `k1`, with no organization or
    /// team binding. Tests narrow one field at a time from here.
    pub(crate) fn caller(user_id: &str, role: &str) -> Caller {
        Caller {
            user_id: user_id.into(),
            user_role: role.into(),
            api_key_id: Some("k1".into()),
            organization_id: None,
            team_id: None,
            grant: None,
            kind: CallerKind::ApiKey,
        }
    }
}
