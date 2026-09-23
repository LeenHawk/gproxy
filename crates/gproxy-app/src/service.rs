//! Vendor CLI services: the endpoints a coding agent calls that are not model
//! traffic — profile, usage, plugins, remote control.
//!
//! The engine already knows how to answer one: it validates the view against
//! the caller's role, picks the credential, refreshes material about to
//! expire, and gives the channel the facts a synthesized answer is rendered
//! from. What it deliberately does *not* know is who is an admin of what — it
//! says so itself — so this module answers exactly two questions and hands the
//! rest over:
//!
//! 1. **which credentials are in the target**: the caller's visible
//!    credentials of that provider, the same set a model call would be allowed
//!    to spend. `Pool` therefore aggregates what this caller may already
//!    reach, and `Credential(id)` can only name one of them;
//! 2. **what role the caller has over that set**: instance administrator, or
//!    an `Admin` member of the organization or team that owns *every*
//!    credential in it. Anything else is a `Member`, and a `Member` may only
//!    ask for the `Caller` view.
//!
//! "Every credential" is not a formality. The target is the whole visible set,
//! so being an admin of one organization must not render a pool that also
//! contains another organization's — or an unowned instance-wide — credential.
//! An unowned credential has no administrator by construction: nobody is an
//! `Admin` member of nothing, so on a single-tenant instance where every
//! credential is shared only an instance administrator reaches `Pool` and
//! `Credential`. That is the conservative end, and widening it is an operator
//! decision (promote the user, or own the credential), not an inference made
//! here.
//!
//! # What a service does not take
//!
//! No rate-limit lease and no usage settlement: services run outside the
//! observation funnel, and charging a window for a profile fetch would spend
//! an allowance the caller needs for the traffic that costs money. The budget
//! chain *is* passed, because the `Caller` view renders the caller's own
//! windows from it — reporting a quota is not spending one.
//!
//! # And no capture, on either side
//!
//! Core says it of its own half — "no attempt record, no usage, no capture" —
//! so a service call leaves no `side = Upstream` row behind. That decides the
//! downstream half too, and not only by symmetry:
//!
//! - there would be nothing to link to. A downstream record whose
//!   `capture_links` are empty claims a request reached no upstream, which for
//!   a service would be a lie the log cannot distinguish from the truth;
//! - there is no [`Admitted`](crate::Admitted). Every attribution column on a
//!   `capture_records` row comes from an admission decision, and a service
//!   takes none — it has a scope, a budget chain and a credential set, decided
//!   here, but it never goes through [`Admission::admit`](crate::Admission);
//! - a service answer is a vendor's profile or usage page, not the caller's
//!   traffic. It is the one thing in this crate a request log is *not* for.
//!
//! So [`App::call_service`] and [`App::connect_service`] open no capture and
//! write no row. A host that wants a trace of them has its own access log,
//! which is where a request that costs nothing and meters nothing belongs.

use std::collections::BTreeSet;

use gproxy_core::{
    BudgetOwner, CallerRole, CoreData, ExecutionTarget, ServiceRequest, ServiceView,
};
use gproxy_protocol::{HttpBody, WireRequest, WireResponse, capability::UpstreamConnection};
use gproxy_seaorm::BatchConnectionTrait;

use crate::{
    App, AppData, AppError, Caller,
    admission::{attribution, budgets, credential, scope},
    snapshot::Owner,
};

/// The picture the client asked for, before the caller's role has been
/// checked against it. The core type it maps to is `ServiceView`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestedView {
    /// The caller's own accounting. The only view a member may ask for.
    Caller,
    /// The target's credentials merged into one synthesized account.
    Pool,
    /// One named credential, forwarded raw. It must be one of the caller's
    /// visible credentials of this provider.
    Credential(String),
}

/// One vendor service request, as the host decoded it.
///
/// `parts` and `body` are forwarded as received — the vendor path is part of
/// the request and the channel matches its own routes against it.
pub struct ServiceRequestIn {
    /// For the operator's log only: a service call is outside the funnel, so
    /// nothing records it under this id.
    pub request_id: String,
    pub parts: http::request::Parts,
    pub body: bytes::Bytes,
    pub view: RequestedView,
    /// The provider the ingress mount named. A service has no model, so there
    /// is nothing to resolve: the host says which provider it is talking to.
    pub provider_id: String,
}

impl std::fmt::Debug for ServiceRequestIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceRequestIn")
            .field("request_id", &self.request_id)
            .field("path", &self.parts.uri.path())
            .field("body_bytes", &self.body.len())
            .field("view", &self.view)
            .field("provider_id", &self.provider_id)
            .finish()
    }
}

/// Everything about a service call that is a decision rather than a lookup.
///
/// Split out from the target so it is a pure function of the snapshot, the
/// caller and the request: the role table and the "same user id as a model
/// request" rule are testable without assembling an engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServicePlan {
    scope: String,
    user_id: Option<String>,
    role: CallerRole,
    view: ServiceView,
    budgets: Vec<BudgetOwner>,
    credentials: BTreeSet<String>,
}

/// The role, the view and the credential set, or the refusal.
///
/// `provider_credentials` is the live credential set of the target provider,
/// as the engine holds it. Visibility narrows it exactly as it does for a
/// model call, so a service can never reach a credential a model request from
/// the same key could not.
pub(crate) fn decide(
    snapshot: &AppData,
    caller: &Caller,
    requested: &RequestedView,
    provider_credentials: &BTreeSet<String>,
) -> Result<ServicePlan, AppError> {
    let credentials = credential::visible_credentials(snapshot, caller, provider_credentials);
    let role = role(snapshot, caller, &credentials);
    let view = match (requested, role) {
        (RequestedView::Caller, _) => ServiceView::Caller,
        (_, CallerRole::Member) => {
            // Core refuses this too; refusing here means it is refused before
            // a credential is read or a refresh is attempted, and the reason
            // is this crate's own vocabulary rather than the engine's.
            return Err(AppError::forbidden(
                "this view is available to administrators of the target credentials only",
            ));
        }
        (RequestedView::Pool, CallerRole::Admin) => ServiceView::Pool,
        (RequestedView::Credential(id), CallerRole::Admin) => {
            if !credentials.contains(id) {
                return Err(AppError::not_found("credential", id));
            }
            ServiceView::Credential(id.clone())
        }
    };
    Ok(ServicePlan {
        scope: scope::render(caller),
        // The same value a model request records, from the same function:
        // core's view of the caller reads it, and two answers to "who is
        // this" inside one instance is the bug the single-`Caller` rule
        // exists to prevent.
        user_id: attribution::attribution(caller, None).user_id,
        role,
        view,
        budgets: budgets::chain(caller),
        credentials,
    })
}

/// The caller's role over one credential set.
///
/// An instance administrator is an administrator of everything — the same
/// bypass credential visibility grants them, for the same reason. Otherwise
/// every credential in the set has to be owned by an organization or a team
/// this caller administers; a set with one credential they do not administer,
/// and an empty set, are both `Member`.
fn role(snapshot: &AppData, caller: &Caller, credentials: &BTreeSet<String>) -> CallerRole {
    if caller.is_instance_admin() {
        return CallerRole::Admin;
    }
    let administered = !credentials.is_empty()
        && credentials.iter().all(|id| {
            match snapshot.credential_ownership.owner(id) {
                Some(Owner::Org(org)) => snapshot.memberships.is_admin_of_org(&caller.user_id, org),
                Some(Owner::Team(team)) => {
                    snapshot.memberships.is_admin_of_team(&caller.user_id, team)
                }
                // A personal credential and an unowned one have no
                // administrator: membership is what confers the role, and
                // neither has a membership behind it.
                Some(Owner::User(_) | Owner::Shared) | None => false,
            }
        });
    if administered {
        CallerRole::Admin
    } else {
        CallerRole::Member
    }
}

impl<C: BatchConnectionTrait + Send + Sync> App<C> {
    /// Call a vendor HTTP service. The channel's answer comes back as it is,
    /// streaming bodies and non-2xx included.
    ///
    /// `Forbidden` when a member asked for an administrator's view,
    /// `NotFound` when the provider or the named credential is not one this
    /// caller can reach.
    pub async fn call_service(
        &self,
        caller: &Caller,
        request: ServiceRequestIn,
    ) -> Result<WireResponse<HttpBody>, AppError> {
        let core = self.gproxy().core().snapshot();
        let (plan, target) = self.prepare_service(&core, caller, &request)?;
        let wire = WireRequest {
            method: request.parts.method.clone(),
            path: request.parts.uri.path().to_owned(),
            query: request.parts.uri.query().map(str::to_owned),
            headers: request.parts.headers,
            body: HttpBody::Bytes(request.body),
        };
        Ok(self
            .gproxy()
            .core()
            .call_service(plan.into_request(target, wire))
            .await?)
    }

    /// Open a vendor websocket service. Same validation and the same target;
    /// a handshake has no body.
    pub async fn connect_service(
        &self,
        caller: &Caller,
        request: ServiceRequestIn,
    ) -> Result<UpstreamConnection, AppError> {
        let core = self.gproxy().core().snapshot();
        let (plan, target) = self.prepare_service(&core, caller, &request)?;
        let wire = WireRequest {
            method: request.parts.method.clone(),
            path: request.parts.uri.path().to_owned(),
            query: request.parts.uri.query().map(str::to_owned),
            headers: request.parts.headers,
            body: (),
        };
        Ok(self
            .gproxy()
            .core()
            .connect_service(plan.into_request(target, wire))
            .await?)
    }

    /// The decision and the target, from one pinned engine snapshot.
    fn prepare_service(
        &self,
        core: &CoreData,
        caller: &Caller,
        request: &ServiceRequestIn,
    ) -> Result<(ServicePlan, ExecutionTarget), AppError> {
        let provider = core
            .providers
            .get(&request.provider_id)
            .ok_or_else(|| AppError::not_found("provider", &request.provider_id))?
            .clone();
        let live: BTreeSet<String> = provider.credential_ids.iter().cloned().collect();
        let plan = decide(&self.data(), caller, &request.view, &live)?;
        let credentials = provider
            .credential_ids
            .iter()
            .filter(|id| plan.credentials.contains(*id))
            .filter_map(|id| core.credentials.get(id).cloned())
            .collect();
        Ok((
            plan,
            ExecutionTarget {
                requested_model: None,
                provider,
                // A service has no model.
                upstream_model: None,
                credentials,
            },
        ))
    }
}

impl ServicePlan {
    fn into_request<B>(
        self,
        target: ExecutionTarget,
        request: WireRequest<B>,
    ) -> ServiceRequest<B> {
        ServiceRequest {
            scope: self.scope,
            user_id: self.user_id,
            caller: self.role,
            view: self.view,
            target,
            budgets: self.budgets,
            request,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::support::caller;
    use gproxy_store::{
        IdentityData,
        entity::{
            identity::{membership_role::MembershipRole, organization_member, team, team_member},
            upstream::credential::{self, CredentialStatus},
        },
    };
    use serde_json::json;

    fn credential_row(
        id: &str,
        user: Option<&str>,
        team: Option<&str>,
        org: Option<&str>,
    ) -> credential::Model {
        credential::Model {
            id: id.into(),
            provider_id: "openai".into(),
            organization_id: org.map(Into::into),
            team_id: team.map(Into::into),
            user_id: user.map(Into::into),
            label: None,
            auth_kind: "api_key".into(),
            secret: Vec::new(),
            version: 0,
            connection_profile_id: None,
            metadata: json!({}),
            expires_at_ms: None,
            status: CredentialStatus::default(),
            status_reason: None,
            enabled: true,
        }
    }

    /// `boss` administers acme, `lead` administers the team `core` inside it,
    /// `member` administers nothing.
    fn snapshot() -> AppData {
        let identity = IdentityData {
            teams: vec![team::Model {
                id: "core".into(),
                organization_id: "acme".into(),
                name: "core".into(),
                oauth_client_allowlist: None,
                created_at_ms: 0,
            }],
            organization_members: vec![
                organization_member::Model {
                    organization_id: "acme".into(),
                    user_id: "boss".into(),
                    role: MembershipRole::Admin,
                },
                organization_member::Model {
                    organization_id: "acme".into(),
                    user_id: "member".into(),
                    role: MembershipRole::Member,
                },
            ],
            team_members: vec![team_member::Model {
                team_id: "core".into(),
                user_id: "lead".into(),
                role: MembershipRole::Admin,
            }],
            ..IdentityData::default()
        };
        AppData::assemble_at(
            1,
            &identity,
            &[
                credential_row("c-shared", None, None, None),
                credential_row("c-boss", Some("boss"), None, None),
                credential_row("c-core", None, Some("core"), None),
                credential_row("c-acme", None, None, Some("acme")),
            ],
            0,
        )
        .unwrap()
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn a_service_reports_the_same_user_as_a_model_request() {
        let mut person = caller("boss", "user");
        person.organization_id = Some("acme".into());
        let plan = decide(
            &snapshot(),
            &person,
            &RequestedView::Caller,
            &set(&["c-acme"]),
        )
        .unwrap();
        assert_eq!(
            plan.user_id,
            attribution::attribution(&person, None).user_id,
            "core's view of the caller has to name the person the usage row names"
        );
        assert_eq!(plan.user_id.as_deref(), Some("boss"));
        assert_eq!(plan.scope, "user:boss");
        assert_eq!(plan.budgets, budgets::chain(&person));
    }

    #[test]
    fn an_organization_admin_administers_that_organizations_credentials() {
        let mut boss = caller("boss", "user");
        boss.organization_id = Some("acme".into());
        let plan = decide(&snapshot(), &boss, &RequestedView::Pool, &set(&["c-acme"])).unwrap();
        assert_eq!(plan.role, CallerRole::Admin);
        assert_eq!(plan.view, ServiceView::Pool);

        // A team admin is not an organization admin, and the reverse holds
        // too: the roles are scoped to the row that grants them.
        let mut lead = caller("lead", "user");
        lead.team_id = Some("core".into());
        let plan = decide(&snapshot(), &lead, &RequestedView::Pool, &set(&["c-core"])).unwrap();
        assert_eq!(plan.role, CallerRole::Admin);
    }

    #[test]
    fn one_unadministered_credential_in_the_target_is_enough_to_be_a_member() {
        let mut boss = caller("boss", "user");
        boss.organization_id = Some("acme".into());
        // The shared credential is visible to everyone and administered by
        // nobody, so the pool it would render is not this admin's to see.
        let refused = decide(
            &snapshot(),
            &boss,
            &RequestedView::Pool,
            &set(&["c-acme", "c-shared"]),
        )
        .unwrap_err();
        assert_eq!(refused.status_code(), 403);
        // The caller view is still theirs.
        let plan = decide(
            &snapshot(),
            &boss,
            &RequestedView::Caller,
            &set(&["c-acme", "c-shared"]),
        )
        .unwrap();
        assert_eq!(plan.role, CallerRole::Member);
        assert_eq!(plan.view, ServiceView::Caller);
    }

    #[test]
    fn an_empty_target_administers_nothing() {
        let mut stranger = caller("stranger", "user");
        stranger.organization_id = Some("globex".into());
        let plan = decide(
            &snapshot(),
            &stranger,
            &RequestedView::Caller,
            &set(&["c-acme"]),
        )
        .unwrap();
        assert!(plan.credentials.is_empty(), "acme's are not visible");
        assert_eq!(plan.role, CallerRole::Member);
    }

    #[test]
    fn an_instance_admin_administers_everything() {
        let root = caller("root", "admin");
        let plan = decide(
            &snapshot(),
            &root,
            &RequestedView::Credential("c-shared".into()),
            &set(&["c-shared", "c-acme"]),
        )
        .unwrap();
        assert_eq!(plan.role, CallerRole::Admin);
        assert_eq!(plan.view, ServiceView::Credential("c-shared".into()));
    }

    #[test]
    fn a_credential_view_can_only_name_one_of_the_targets_own() {
        let root = caller("root", "admin");
        let missing = decide(
            &snapshot(),
            &root,
            &RequestedView::Credential("c-core".into()),
            &set(&["c-shared"]),
        )
        .unwrap_err();
        assert_eq!(missing.status_code(), 404);
    }

    #[test]
    fn a_member_asking_for_an_administrators_view_is_refused() {
        let member = caller("member", "user");
        for view in [
            RequestedView::Pool,
            RequestedView::Credential("c-shared".into()),
        ] {
            let refused = decide(&snapshot(), &member, &view, &set(&["c-shared"])).unwrap_err();
            assert_eq!(refused.status_code(), 403);
            assert_eq!(refused.code(), "forbidden");
        }
    }
}
