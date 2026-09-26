//! Organization- and team-scoped administration: one management API, one
//! typed scope per request.
//!
//! # Why a scope rather than a second API
//!
//! `organization_members.role` and `team_members.role` have held
//! [`MembershipRole::Admin`] since the schema was written and nothing consumed
//! it: `/admin/api` demanded an instance administrator and `/portal/api` is
//! strictly self-service. An organization administrator had no surface at all.
//!
//! They now have the *same* surface. The caller carries an [`AdminScope`], an
//! organization administrator calls the same routes as the operator, and what
//! changes is how many rows come back. Two management APIs would be two places
//! for a rule to live and one of them would drift.
//!
//! # The property this module exists to hold
//!
//! The portal is safe because there is no parameter through which a caller
//! could name somebody else. This surface must be safe for the mirror-image
//! reason: **there is no scope through which a caller could reach outside
//! their own**, because the scope participates in building the query rather
//! than being compared against the answer.
//!
//! Concretely that is three functions and nothing else:
//!
//! - [`AdminScope::resolve`] — the *only* place a scope is derived, called
//!   once per request by the host's guard;
//! - [`AdminScope::narrow`] — a list's owner filter, rewritten so the database
//!   never sees a query that could match a foreign row;
//! - [`AdminScope::admits`] — one row's owner, for the routes that take an id
//!   and therefore cannot be narrowed.
//!
//! A per-handler `if` is exactly what this must not become. Nothing outside
//! this module compares an organization id to a caller.
//!
//! # Where a scope comes from
//!
//! | caller | scope |
//! |---|---|
//! | `users.role = admin` | [`AdminScope::Instance`]; the header is not read |
//! | an OAuth access token | none: refused, even when an administrator authorized it |
//! | an API key | the key's own `team_id`, else its `organization_id`, if the key's owner administers it |
//! | a console session | the `x-gproxy-admin-scope` header, validated against the caller's **admin** memberships |
//!
//! The API key case is deliberate and is the same rule the data plane already
//! follows: the key's binding decides budget attribution, the permission
//! subject and credential visibility, so it decides this too. Nothing a client
//! sends can change it — a header on a key request is ignored, not refused,
//! because refusing would make a console that sets the header globally fail on
//! exactly the callers where the header cannot matter.
//!
//! A session caller who administers exactly one organization or team does not
//! need the header; one who administers several must name which, and one who
//! administers none is refused the whole surface, as they were before this
//! module existed.
//!
//! # Rows outside the scope are `NotFound`
//!
//! Never `Forbidden`. A `Forbidden` confirms the id exists, which is the fact
//! an enumeration is looking for; from outside, another organization's
//! credential and an id that was never minted must be the same answer. This is
//! the portal's rule, applied to the other surface.

use gproxy_sdk::{Gproxy, dto::ListQuery as ManageQuery};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::membership_role::MembershipRole;

use crate::{AppData, AppError, Caller, CallerKind, Result};

/// The header a console session names its scope with.
///
/// `x-` prefixed and namespaced to this product, so it cannot collide with
/// anything a proxy in front of the instance sets. Values are the same
/// spellings [`AdminScope::selector`] renders: `instance`,
/// `organization:{id}`, `team:{id}`.
pub const SCOPE_HEADER: &str = "x-gproxy-admin-scope";

/// The one refusal message this module produces for a scope a caller may not
/// act as.
///
/// A single string on purpose: "you do not administer that organization" and
/// "that organization does not exist" have to be the same answer, or the
/// header becomes an organization enumerator.
const NO_SCOPE: &str = "this caller administers nothing on this instance";

/// The refusal for an OAuth access token. It names the reason, unlike
/// [`NO_SCOPE`]: whether a token may administer is not a secret about any row.
const OAUTH_REFUSED: &str =
    "an OAuth access token may not administer; sign in to the console instead";

/// What one request may act as. Exactly one per request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminScope {
    /// The operator. Every row of every family.
    Instance,
    /// One organization: its own rows and its teams'.
    Organization(String),
    /// One team.
    Team(String),
}

/// The owner of one row, as a scope reads it.
///
/// Four cases because that is what the owner columns can say. A row with no
/// owner at all is **instance machinery** — a shared credential, an operator
/// limit on a provider — and belongs to nobody but the operator, which is why
/// it is not the same thing as "owned by the organization".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeOwner<'a> {
    /// No owner column set.
    Instance,
    User(&'a str),
    Team(&'a str),
    Organization(&'a str),
}

impl<'a> ScopeOwner<'a> {
    /// The three owner columns as one owner, narrowest first.
    ///
    /// The same precedence `CredentialOwnership::build` uses, and for the same
    /// reason: management sets exactly one of the three, and when a
    /// hand-edited row nevertheless has several, the narrowest reading is the
    /// one that leaks least.
    pub fn from_columns(
        user_id: Option<&'a str>,
        team_id: Option<&'a str>,
        organization_id: Option<&'a str>,
    ) -> Self {
        match (user_id, team_id, organization_id) {
            (Some(user), _, _) => Self::User(user),
            (None, Some(team), _) => Self::Team(team),
            (None, None, Some(organization)) => Self::Organization(organization),
            (None, None, None) => Self::Instance,
        }
    }

    /// The `(owner_kind, owner_id)` pair the `quotas` table stores, as an
    /// owner.
    ///
    /// The budget kinds are the five `admission::attribution` writes — and
    /// `credential` and `provider`, which are operator limits sharing the
    /// table. Everything this scope model does not name is `Instance`: a
    /// `user` or `api_key` budget is a person's, not an
    /// organization's, and an operator limit is machinery. The split between a
    /// budget and a limit is by `owner_kind`, not by route, so the same
    /// function answers for both.
    ///
    /// A `credential` limit reads as `Instance` here only because this
    /// function cannot see the credential; [`AdminScope::admits_quota_owner`]
    /// resolves it through the credential's owner.
    pub fn from_pair(owner_kind: &str, owner_id: &'a str) -> Self {
        match owner_kind {
            "org" => Self::Organization(owner_id),
            "team" => Self::Team(owner_id),
            "user" => Self::User(owner_id),
            _ => Self::Instance,
        }
    }

    /// The `(owner_kind, owner_id)` pair this owner is stored as, where one
    /// exists. `Instance` has none: it is the absence of an owner.
    pub fn pair(&self) -> Option<(&'static str, &'a str)> {
        match self {
            Self::Instance => None,
            Self::User(id) => Some(("user", id)),
            Self::Team(id) => Some(("team", id)),
            Self::Organization(id) => Some(("org", id)),
        }
    }
}

/// A list query after the scope has been applied to it.
///
/// `Nothing` is not an error: a caller that filtered a list down to an owner
/// outside its scope asked a well-formed question whose answer is empty. An
/// error there would tell them the difference between "no rows" and "not
/// yours", which is the same disclosure a `Forbidden` on a single row would
/// be.
///
/// The query is boxed because it is by far the larger of the two variants and
/// this value is returned by copy from every list on the surface.
#[derive(Debug, Clone)]
pub enum ScopedQuery {
    Run(Box<ManageQuery>),
    Nothing,
}

impl AdminScope {
    /// The header spelling of this scope, which is also what
    /// [`AdminScope::parse`] reads back.
    pub fn selector(&self) -> String {
        match self {
            Self::Instance => "instance".to_owned(),
            Self::Organization(id) => format!("organization:{id}"),
            Self::Team(id) => format!("team:{id}"),
        }
    }

    /// One of `instance`, `organization:{id}` (or `org:{id}`), `team:{id}`.
    ///
    /// A bare id is deliberately not accepted: an organization id and a team
    /// id are drawn from the same alphabet, so a caller that meant one and got
    /// the other would be silently scoped to something they did administer,
    /// which is worse than a rejection.
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("instance") {
            return Ok(Self::Instance);
        }
        let (kind, id) = value.split_once(':').ok_or_else(|| {
            AppError::invalid(format!(
                "`{SCOPE_HEADER}` must be `instance`, `organization:{{id}}` or `team:{{id}}`"
            ))
        })?;
        let id = id.trim();
        if id.is_empty() {
            return Err(AppError::invalid(format!("`{SCOPE_HEADER}` names no id")));
        }
        match kind.trim() {
            "organization" | "org" => Ok(Self::Organization(id.to_owned())),
            "team" => Ok(Self::Team(id.to_owned())),
            other => Err(AppError::invalid(format!(
                "`{SCOPE_HEADER}` kind `{other}` is not one of instance, organization, team"
            ))),
        }
    }

    pub fn is_instance(&self) -> bool {
        matches!(self, Self::Instance)
    }

    /// The organization this scope acts in, which for a team is its parent.
    pub fn organization<'a>(&'a self, data: &'a AppData) -> Option<&'a str> {
        match self {
            Self::Instance => None,
            Self::Organization(id) => Some(id.as_str()),
            Self::Team(id) => data.teams.get(id).map(|team| team.organization_id.as_str()),
        }
    }

    /// Whether a row owned by `owner` is inside this scope.
    ///
    /// The whole containment rule, in one place:
    ///
    /// - the instance scope contains everything, including unowned rows;
    /// - an organization contains its own rows and those of its teams, and
    ///   **not** its members' personal rows — a member's own credential is
    ///   theirs, and the portal is where they manage it;
    /// - a team contains only its own.
    pub fn admits(&self, owner: ScopeOwner<'_>, data: &AppData) -> bool {
        match self {
            Self::Instance => true,
            Self::Organization(scope) => match owner {
                ScopeOwner::Organization(id) => id == scope,
                ScopeOwner::Team(id) => data
                    .teams
                    .get(id)
                    .is_some_and(|team| team.organization_id == *scope),
                ScopeOwner::User(_) | ScopeOwner::Instance => false,
            },
            Self::Team(scope) => matches!(owner, ScopeOwner::Team(id) if id == scope),
        }
    }

    /// `owner` as a row this scope may reach, or `NotFound` naming `entity`
    /// and `id`.
    ///
    /// Deliberately not `Forbidden`: see the module note.
    pub fn admit(
        &self,
        owner: ScopeOwner<'_>,
        data: &AppData,
        entity: &'static str,
        id: &str,
    ) -> Result<()> {
        if self.admits(owner, data) {
            return Ok(());
        }
        Err(AppError::not_found(entity, id))
    }

    /// The same, for a row a caller is about to **write**.
    ///
    /// A create names its own owner, so this is where "an organization
    /// administrator cannot create a row naming an owner outside their scope"
    /// is enforced. It is `Forbidden` rather than `NotFound` because the
    /// caller supplied the owner rather than discovered it: there is nothing
    /// to leak about the existence of an id they typed themselves, and
    /// `NotFound` would read as "the organization does not exist" on a create
    /// that names one that plainly does.
    pub fn admit_write(&self, owner: ScopeOwner<'_>, data: &AppData) -> Result<()> {
        if self.admits(owner, data) {
            return Ok(());
        }
        Err(AppError::forbidden(match owner.pair() {
            Some((kind, id)) => format!("`{kind}:{id}` is outside this scope"),
            None => "an unowned row is instance machinery".to_owned(),
        }))
    }

    /// One list query, with its owner filter replaced by the intersection of
    /// what the caller asked for and what the scope permits.
    ///
    /// This is the narrowing that makes a list safe by construction. The rules:
    ///
    /// - the instance scope rewrites nothing;
    /// - a query that names no owner is given every owner the scope contains —
    ///   so the default view of an organization administrator is their
    ///   organization's rows and its teams';
    /// - a query that names an owner inside the scope keeps it, which is how
    ///   an organization administrator reaches one of their teams' rows;
    /// - a query that names anything else becomes [`ScopedQuery::Nothing`].
    ///
    /// The filter is the `(ownerKind, ownerId)` pair the sdk's families
    /// already understand, so there is no second filtering path to keep in
    /// step with the first.
    pub fn narrow(&self, mut query: ManageQuery, data: &AppData) -> ScopedQuery {
        let Some((kind, id)) = self.own_pair() else {
            return ScopedQuery::Run(Box::new(query));
        };
        match (
            trimmed(query.owner_kind.as_deref()),
            trimmed(query.owner_id.as_deref()),
        ) {
            (Some(asked_kind), Some(asked_id)) => {
                if self.admits(ScopeOwner::from_pair(asked_kind, asked_id), data) {
                    ScopedQuery::Run(Box::new(query))
                } else {
                    ScopedQuery::Nothing
                }
            }
            // Half a pair is not a filter: an `ownerId` with no kind would
            // match a team id against an organization row. Replace both.
            _ => {
                query.owner_kind = None;
                query.owner_id = None;
                query.owner_any = self.own_pairs(kind, id, data);
                ScopedQuery::Run(Box::new(query))
            }
        }
    }

    /// Every owner pair this scope contains: itself and, for an organization,
    /// each of its teams — the same set [`AdminScope::admits`] accepts.
    fn own_pairs(&self, kind: &str, id: &str, data: &AppData) -> Vec<(String, String)> {
        let mut pairs = vec![(kind.to_owned(), id.to_owned())];
        if let Self::Organization(organization) = self {
            pairs.extend(
                data.teams
                    .values()
                    .filter(|team| team.organization_id == *organization)
                    .map(|team| ("team".to_owned(), team.id.clone())),
            );
        }
        pairs
    }

    /// The owner pair this scope *is*, or None for the instance scope.
    fn own_pair(&self) -> Option<(&'static str, &str)> {
        match self {
            Self::Instance => None,
            Self::Organization(id) => Some(("org", id)),
            Self::Team(id) => Some(("team", id)),
        }
    }

    /// Every scope `caller` may act as, and the one they are acting as now.
    ///
    /// The single derivation site. A host's guard calls this once per request
    /// and puts the answer in the request; nothing else in this workspace
    /// constructs an [`AdminScope`] from a caller.
    ///
    /// `requested` is the raw `x-gproxy-admin-scope` value, if the request
    /// carried one. It is read **only** for a session caller who is not an
    /// instance administrator; see the module table.
    pub fn resolve(
        caller: &Caller,
        data: &AppData,
        requested: Option<&str>,
    ) -> Result<AdminAdmission> {
        // Before the role check: a token issued to a third-party program is
        // not an administrative credential, whoever authorized it. The same
        // rule `check_oauth_operation` holds on the data plane.
        if caller.kind == CallerKind::OAuthGrant {
            return Err(AppError::forbidden(OAUTH_REFUSED));
        }
        if caller.is_instance_admin() {
            return Ok(AdminAdmission {
                available: vec![Self::Instance],
                current: Some(Self::Instance),
            });
        }
        // A key decides its own scope. Team first: it is the narrower of the
        // two bindings, and a key bound to both is a team key whose parent
        // organization is reachable through the team.
        if caller.kind == CallerKind::ApiKey {
            let bound = caller
                .team_id
                .as_deref()
                .map(|id| Self::Team(id.to_owned()))
                .or_else(|| {
                    caller
                        .organization_id
                        .as_deref()
                        .map(|id| Self::Organization(id.to_owned()))
                });
            // The binding only picks which scope; the key's owner must still
            // administer it. Any member may mint a key bound to their
            // organization, so without this a plain member's key would be an
            // organization administrator's.
            let scope = bound
                .filter(|scope| scope.is_administered_by(&caller.user_id, data))
                .ok_or_else(|| AppError::forbidden(NO_SCOPE))?;
            return Ok(AdminAdmission {
                available: vec![scope.clone()],
                current: Some(scope),
            });
        }
        let available = administered(caller, data);
        if available.is_empty() {
            return Err(AppError::forbidden(NO_SCOPE));
        }
        let current = match requested.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => {
                let asked = Self::parse(value)?;
                if !available.contains(&asked) {
                    return Err(AppError::forbidden(NO_SCOPE));
                }
                Some(asked)
            }
            // One scope is not a choice, so it need not be made. Several is,
            // and guessing would silently act on the wrong organization.
            None if available.len() == 1 => available.first().cloned(),
            None => None,
        };
        Ok(AdminAdmission { available, current })
    }
}

impl AdminScope {
    /// Whether `user_id` holds the admin role this scope needs right now.
    ///
    /// A team is administered by its own admins and by its organization's:
    /// the organization scope contains the team's rows, so a team-bound key
    /// of an organization administrator reaches nothing they could not already.
    fn is_administered_by(&self, user_id: &str, data: &AppData) -> bool {
        match self {
            Self::Instance => false,
            Self::Organization(id) => data.memberships.is_admin_of_org(user_id, id),
            Self::Team(id) => {
                data.memberships.is_admin_of_team(user_id, id)
                    || data.teams.get(id).is_some_and(|team| {
                        data.memberships
                            .is_admin_of_org(user_id, &team.organization_id)
                    })
            }
        }
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The scopes a session caller administers, organizations before teams.
///
/// An organization administrator is not implicitly an administrator of its
/// teams, exactly as [`MembershipIndex::is_admin_of_team`] says — but the
/// organization scope *contains* its teams' rows, so they do not need a team
/// scope to reach them. The two facts are consistent: the role is scoped to
/// the row that grants it, and the containment is a property of the rows.
///
/// [`MembershipIndex::is_admin_of_team`]: crate::snapshot::MembershipIndex::is_admin_of_team
fn administered(caller: &Caller, data: &AppData) -> Vec<AdminScope> {
    let mut out: Vec<AdminScope> = data
        .memberships
        .organizations_of(&caller.user_id)
        .iter()
        .filter(|(_, role)| *role == MembershipRole::Admin)
        .map(|(id, _)| AdminScope::Organization(id.clone()))
        .collect();
    for (team_id, role) in data.memberships.teams_of(&caller.user_id) {
        if *role != MembershipRole::Admin {
            continue;
        }
        // A team whose organization the caller already administers adds
        // nothing: the organization scope is strictly wider.
        let covered = data.teams.get(team_id).is_some_and(|team| {
            out.contains(&AdminScope::Organization(team.organization_id.clone()))
        });
        if !covered {
            out.push(AdminScope::Team(team_id.clone()));
        }
    }
    out
}

/// What a caller may act as, and what they are acting as.
///
/// `current` is `None` only for a session caller who administers several
/// scopes and named none of them. Every route but `GET /admin/api/context`
/// needs a scope, so the host asks for one through
/// [`AdminAdmission::require`]; the context route renders `available` so the
/// console can offer the choice.
#[derive(Clone, Debug)]
pub struct AdminAdmission {
    pub available: Vec<AdminScope>,
    pub current: Option<AdminScope>,
}

impl AdminAdmission {
    /// The scope this request acts as, or the refusal that names the header.
    pub fn require(&self) -> Result<&AdminScope> {
        self.current.as_ref().ok_or_else(|| {
            AppError::invalid(format!(
                "this caller administers several scopes; name one with `{SCOPE_HEADER}`"
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// The families whose rows carry an owner, and the reads that admit one.
// ---------------------------------------------------------------------------

impl AdminScope {
    /// Whether the credential behind `id` is inside this scope, as
    /// `NotFound` when it is not.
    ///
    /// The snapshot answers first — [`CredentialOwnership`] already holds
    /// exactly this mapping, so the common case costs no query at all. A miss
    /// falls back to a read for the same reason authentication does: a
    /// credential created by the request before this one is a revision newer
    /// than the snapshot, and refusing it would be a race, not a decision.
    ///
    /// [`CredentialOwnership`]: crate::snapshot::CredentialOwnership
    pub async fn admit_credential<C>(
        &self,
        gproxy: &Gproxy<C>,
        data: &AppData,
        id: &str,
    ) -> Result<()>
    where
        C: BatchConnectionTrait + Send + Sync + 'static,
    {
        if self.admits_credential(gproxy, data, id).await? {
            return Ok(());
        }
        Err(AppError::not_found("credential", id))
    }

    /// The same as a bool: false for a credential outside the scope and for
    /// one that does not exist, which from outside are the same thing.
    pub async fn admits_credential<C>(
        &self,
        gproxy: &Gproxy<C>,
        data: &AppData,
        id: &str,
    ) -> Result<bool>
    where
        C: BatchConnectionTrait + Send + Sync + 'static,
    {
        if self.is_instance() {
            return Ok(true);
        }
        use crate::snapshot::Owner;
        let owner = match data.credential_ownership.owner(id) {
            Some(Owner::Shared) => ScopeOwner::Instance,
            Some(Owner::User(user)) => ScopeOwner::User(user),
            Some(Owner::Team(team)) => ScopeOwner::Team(team),
            Some(Owner::Org(organization)) => ScopeOwner::Organization(organization),
            None => {
                let Some(row) = gproxy
                    .store()
                    .credentials()
                    .get_many(&[id.to_owned()])
                    .await?
                    .into_iter()
                    .next()
                    .flatten()
                else {
                    return Ok(false);
                };
                return Ok(self.admits(
                    ScopeOwner::from_columns(
                        row.user_id.as_deref(),
                        row.team_id.as_deref(),
                        row.organization_id.as_deref(),
                    ),
                    data,
                ));
            }
        };
        Ok(self.admits(owner, data))
    }

    /// Whether a `quotas` row owned by `(owner_kind, owner_id)` is inside this
    /// scope.
    ///
    /// A `credential` limit belongs to whoever owns the credential: whoever
    /// may add the credential may also cap it. Every other kind is decided by
    /// [`ScopeOwner::from_pair`] alone.
    pub async fn admits_quota_owner<C>(
        &self,
        gproxy: &Gproxy<C>,
        data: &AppData,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<bool>
    where
        C: BatchConnectionTrait + Send + Sync + 'static,
    {
        if owner_kind == "credential" {
            return self.admits_credential(gproxy, data, owner_id).await;
        }
        Ok(self.admits(ScopeOwner::from_pair(owner_kind, owner_id), data))
    }

    /// The same for a `quotas` row, which is not in any snapshot and is
    /// therefore always read.
    ///
    /// The row is returned so a caller that also has to admit a *patched*
    /// owner does not read it twice.
    pub async fn admit_quota<C>(
        &self,
        gproxy: &Gproxy<C>,
        data: &AppData,
        id: &str,
    ) -> Result<gproxy_store::entity::limits::quota::Model>
    where
        C: BatchConnectionTrait + Send + Sync + 'static,
    {
        let row = gproxy
            .store()
            .quotas()
            .get_many(&[id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| AppError::not_found("quota", id))?;
        if !self
            .admits_quota_owner(gproxy, data, &row.owner_kind, &row.owner_id)
            .await?
        {
            return Err(AppError::not_found("quota", id));
        }
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::AppData;
    use gproxy_store::entity::identity::{organization, organization_member, team, team_member};

    fn app_data() -> AppData {
        let mut data = AppData::empty();
        data.organizations.insert(
            "acme".into(),
            organization::Model {
                id: "acme".into(),
                name: "Acme".into(),
                oauth_client_allowlist: None,
                created_at_ms: 0,
            },
        );
        data.organizations.insert(
            "globex".into(),
            organization::Model {
                id: "globex".into(),
                name: "Globex".into(),
                oauth_client_allowlist: None,
                created_at_ms: 0,
            },
        );
        for (id, organization) in [("core", "acme"), ("ops", "globex")] {
            data.teams.insert(
                id.into(),
                team::Model {
                    id: id.into(),
                    organization_id: organization.into(),
                    name: id.into(),
                    oauth_client_allowlist: None,
                    created_at_ms: 0,
                },
            );
        }
        data.memberships = crate::snapshot::MembershipIndex::build(
            &[
                organization_member::Model {
                    organization_id: "acme".into(),
                    user_id: "orgadmin".into(),
                    role: MembershipRole::Admin,
                },
                organization_member::Model {
                    organization_id: "acme".into(),
                    user_id: "member".into(),
                    role: MembershipRole::Member,
                },
                organization_member::Model {
                    organization_id: "globex".into(),
                    user_id: "both".into(),
                    role: MembershipRole::Admin,
                },
                organization_member::Model {
                    organization_id: "acme".into(),
                    user_id: "both".into(),
                    role: MembershipRole::Admin,
                },
            ],
            &[
                team_member::Model {
                    team_id: "core".into(),
                    user_id: "lead".into(),
                    role: MembershipRole::Admin,
                },
                // `orgadmin` already administers acme, so this adds nothing.
                team_member::Model {
                    team_id: "core".into(),
                    user_id: "orgadmin".into(),
                    role: MembershipRole::Admin,
                },
            ],
            &data.teams.values().cloned().collect::<Vec<_>>(),
        );
        data
    }

    fn caller(user: &str, role: &str, kind: CallerKind) -> Caller {
        Caller {
            user_id: user.into(),
            user_role: role.into(),
            api_key_id: matches!(kind, CallerKind::ApiKey).then(|| "k1".to_owned()),
            organization_id: None,
            team_id: None,
            grant: None,
            kind,
        }
    }

    #[test]
    fn a_selector_round_trips() {
        for scope in [
            AdminScope::Instance,
            AdminScope::Organization("acme".into()),
            AdminScope::Team("core".into()),
        ] {
            assert_eq!(AdminScope::parse(&scope.selector()).unwrap(), scope);
        }
        assert_eq!(
            AdminScope::parse("org:acme").unwrap(),
            AdminScope::Organization("acme".into())
        );
        // A bare id, a blank id and an unknown kind are all refusals.
        assert_eq!(AdminScope::parse("acme").unwrap_err().status_code(), 400);
        assert_eq!(AdminScope::parse("org:").unwrap_err().status_code(), 400);
        assert_eq!(AdminScope::parse("pool:p1").unwrap_err().status_code(), 400);
    }

    #[test]
    fn an_organization_contains_its_own_rows_and_its_teams() {
        let data = app_data();
        let scope = AdminScope::Organization("acme".into());
        assert!(scope.admits(ScopeOwner::Organization("acme"), &data));
        assert!(scope.admits(ScopeOwner::Team("core"), &data));
        assert!(!scope.admits(ScopeOwner::Team("ops"), &data));
        assert!(!scope.admits(ScopeOwner::Organization("globex"), &data));
        // A member's personal row is theirs, and an unowned row is the
        // operator's.
        assert!(!scope.admits(ScopeOwner::User("member"), &data));
        assert!(!scope.admits(ScopeOwner::Instance, &data));
    }

    #[test]
    fn a_team_contains_only_itself_and_the_instance_contains_everything() {
        let data = app_data();
        let team = AdminScope::Team("core".into());
        assert!(team.admits(ScopeOwner::Team("core"), &data));
        assert!(!team.admits(ScopeOwner::Organization("acme"), &data));
        assert!(!team.admits(ScopeOwner::Instance, &data));
        for owner in [
            ScopeOwner::Instance,
            ScopeOwner::User("member"),
            ScopeOwner::Team("ops"),
            ScopeOwner::Organization("globex"),
        ] {
            assert!(AdminScope::Instance.admits(owner, &data));
        }
    }

    #[test]
    fn an_operator_limit_is_never_inside_an_organization() {
        let data = app_data();
        let scope = AdminScope::Organization("acme".into());
        // The `quotas` table holds budgets and operator limits side by side;
        // the split is by owner kind, and these two are machinery.
        for kind in ["credential", "provider", "api_key"] {
            assert!(!scope.admits(ScopeOwner::from_pair(kind, "anything"), &data));
        }
        assert!(scope.admits(ScopeOwner::from_pair("org", "acme"), &data));
        assert!(scope.admits(ScopeOwner::from_pair("team", "core"), &data));
    }

    #[test]
    fn a_list_is_narrowed_to_the_scope_rather_than_checked_after() {
        let data = app_data();
        let scope = AdminScope::Organization("acme".into());

        /// The owner pair a narrowing produced, or None when it matches
        /// nothing.
        fn pair(narrowed: ScopedQuery) -> Option<(Option<String>, Option<String>)> {
            match narrowed {
                ScopedQuery::Nothing => None,
                ScopedQuery::Run(query) => Some((query.owner_kind, query.owner_id)),
            }
        }

        /// The any-of owner set a narrowing produced.
        fn any(narrowed: ScopedQuery) -> Vec<(String, String)> {
            match narrowed {
                ScopedQuery::Nothing => Vec::new(),
                ScopedQuery::Run(query) => query.owner_any,
            }
        }
        let owned = |kind: &str, id: &str| (kind.to_owned(), id.to_owned());

        // No owner named: the organization and each of its teams, and nothing
        // of another organization's.
        let narrowed = scope.narrow(ManageQuery::default(), &data);
        assert_eq!(
            any(narrowed.clone()),
            vec![owned("org", "acme"), owned("team", "core")]
        );
        assert_eq!(pair(narrowed), Some((None, None)));

        // A team inside the organization survives untouched.
        let asked = ManageQuery {
            owner_kind: Some("team".into()),
            owner_id: Some("core".into()),
            ..ManageQuery::default()
        };
        assert_eq!(
            pair(scope.narrow(asked, &data)),
            Some((Some("team".into()), Some("core".into())))
        );

        // Anything else matches nothing rather than erroring.
        let foreign = ManageQuery {
            owner_kind: Some("org".into()),
            owner_id: Some("globex".into()),
            ..ManageQuery::default()
        };
        assert_eq!(pair(scope.narrow(foreign, &data)), None);
        let limit = ManageQuery {
            owner_kind: Some("credential".into()),
            owner_id: Some("c1".into()),
            ..ManageQuery::default()
        };
        assert_eq!(pair(scope.narrow(limit, &data)), None);

        // Half a pair is not a filter and is replaced outright.
        let half = ManageQuery {
            owner_id: Some("globex".into()),
            ..ManageQuery::default()
        };
        let narrowed = scope.narrow(half, &data);
        assert_eq!(pair(narrowed.clone()), Some((None, None)));
        assert_eq!(
            any(narrowed),
            vec![owned("org", "acme"), owned("team", "core")]
        );

        // A team scope can only ever be itself.
        let team = AdminScope::Team("core".into());
        assert_eq!(
            any(team.narrow(ManageQuery::default(), &data)),
            vec![owned("team", "core")]
        );
        let parent = ManageQuery {
            owner_kind: Some("org".into()),
            owner_id: Some("acme".into()),
            ..ManageQuery::default()
        };
        assert_eq!(pair(team.narrow(parent, &data)), None);

        // The instance scope rewrites nothing.
        let untouched = ManageQuery {
            owner_kind: Some("provider".into()),
            owner_id: Some("p1".into()),
            ..ManageQuery::default()
        };
        assert_eq!(
            pair(AdminScope::Instance.narrow(untouched, &data)),
            Some((Some("provider".into()), Some("p1".into())))
        );
        assert_eq!(
            pair(AdminScope::Instance.narrow(ManageQuery::default(), &data)),
            Some((None, None))
        );
    }

    #[test]
    fn an_instance_administrator_is_the_instance_scope_and_ignores_the_header() {
        let data = app_data();
        let caller = caller("root", "admin", CallerKind::Session);
        let admission = AdminScope::resolve(&caller, &data, Some("org:globex")).unwrap();
        assert_eq!(admission.current, Some(AdminScope::Instance));
        assert_eq!(admission.available, vec![AdminScope::Instance]);
    }

    #[test]
    fn a_keys_binding_wins_over_any_header_it_sends() {
        let data = app_data();
        let mut key = caller("orgadmin", "user", CallerKind::ApiKey);
        key.organization_id = Some("acme".into());
        let admission = AdminScope::resolve(&key, &data, Some("org:globex")).unwrap();
        assert_eq!(
            admission.current,
            Some(AdminScope::Organization("acme".into()))
        );

        // A team binding is the narrower one and wins over the organization.
        key.team_id = Some("core".into());
        let admission = AdminScope::resolve(&key, &data, Some("instance")).unwrap();
        assert_eq!(admission.current, Some(AdminScope::Team("core".into())));

        // A key bound to nothing administers nothing.
        let plain = caller("orgadmin", "user", CallerKind::ApiKey);
        assert_eq!(
            AdminScope::resolve(&plain, &data, None)
                .unwrap_err()
                .status_code(),
            403
        );
    }

    #[test]
    fn a_key_binding_needs_its_owner_to_administer_it() {
        let data = app_data();
        // Any member may mint a key bound to their organization; that key is
        // not an administrator's.
        let mut key = caller("member", "user", CallerKind::ApiKey);
        key.organization_id = Some("acme".into());
        assert_eq!(
            AdminScope::resolve(&key, &data, None)
                .unwrap_err()
                .status_code(),
            403
        );
        key.team_id = Some("core".into());
        assert_eq!(
            AdminScope::resolve(&key, &data, None)
                .unwrap_err()
                .status_code(),
            403
        );

        // A team key is administered by the team's admin and by its
        // organization's.
        for user in ["lead", "orgadmin"] {
            let mut key = caller(user, "user", CallerKind::ApiKey);
            key.organization_id = Some("acme".into());
            key.team_id = Some("core".into());
            assert_eq!(
                AdminScope::resolve(&key, &data, None).unwrap().current,
                Some(AdminScope::Team("core".into())),
                "{user}"
            );
        }
        // A team admin's organization-bound key is not an organization
        // administrator's.
        let mut key = caller("lead", "user", CallerKind::ApiKey);
        key.organization_id = Some("acme".into());
        assert_eq!(
            AdminScope::resolve(&key, &data, None)
                .unwrap_err()
                .status_code(),
            403
        );
    }

    #[test]
    fn a_session_caller_with_one_scope_needs_no_header() {
        let data = app_data();
        let admission = AdminScope::resolve(
            &caller("orgadmin", "user", CallerKind::Session),
            &data,
            None,
        )
        .unwrap();
        assert_eq!(
            admission.current,
            Some(AdminScope::Organization("acme".into()))
        );
        // The team admin role on `core` is covered by the acme scope, so it is
        // not offered a second time.
        assert_eq!(admission.available.len(), 1);

        let lead =
            AdminScope::resolve(&caller("lead", "user", CallerKind::Session), &data, None).unwrap();
        assert_eq!(lead.current, Some(AdminScope::Team("core".into())));
    }

    #[test]
    fn a_session_caller_with_several_scopes_must_name_one() {
        let data = app_data();
        let caller = caller("both", "user", CallerKind::Session);
        let admission = AdminScope::resolve(&caller, &data, None).unwrap();
        assert_eq!(admission.available.len(), 2);
        assert!(admission.current.is_none());
        // Reaching a route without naming one is a 400 that names the header.
        let error = admission.require().unwrap_err();
        assert_eq!(error.status_code(), 400);
        assert!(error.to_string().contains(SCOPE_HEADER));

        let named = AdminScope::resolve(&caller, &data, Some("org:globex")).unwrap();
        assert_eq!(
            named.current,
            Some(AdminScope::Organization("globex".into()))
        );
    }

    #[test]
    fn a_header_naming_a_scope_the_caller_does_not_administer_is_refused() {
        let data = app_data();
        let caller = caller("orgadmin", "user", CallerKind::Session);
        // Another organization, a team the caller does not lead, the instance
        // scope, and an organization that does not exist all answer the same
        // way: the header must not become an enumerator.
        for asked in ["org:globex", "team:ops", "instance", "org:nonexistent"] {
            let error = AdminScope::resolve(&caller, &data, Some(asked)).unwrap_err();
            assert_eq!(error.status_code(), 403, "{asked}");
            assert!(error.to_string().contains(NO_SCOPE), "{asked}");
        }
    }

    #[test]
    fn a_plain_member_administers_nothing() {
        let data = app_data();
        let error =
            AdminScope::resolve(&caller("member", "user", CallerKind::Session), &data, None)
                .unwrap_err();
        assert_eq!(error.status_code(), 403);
        let error = AdminScope::resolve(
            &caller("stranger", "user", CallerKind::Session),
            &data,
            None,
        )
        .unwrap_err();
        assert_eq!(error.status_code(), 403);
    }

    #[test]
    fn an_oauth_token_administers_nothing_whoever_authorized_it() {
        let data = app_data();
        let admin = caller("root", "admin", CallerKind::OAuthGrant);
        let error = AdminScope::resolve(&admin, &data, None).unwrap_err();
        assert_eq!(error.status_code(), 403);

        let mut org_admin = caller("orgadmin", "user", CallerKind::OAuthGrant);
        org_admin.organization_id = Some("acme".into());
        let error = AdminScope::resolve(&org_admin, &data, None).unwrap_err();
        assert_eq!(error.status_code(), 403);
    }

    #[test]
    fn a_row_outside_the_scope_is_not_found_but_a_write_naming_one_is_forbidden() {
        let data = app_data();
        let scope = AdminScope::Organization("acme".into());
        let error = scope
            .admit(ScopeOwner::Organization("globex"), &data, "quota", "q1")
            .unwrap_err();
        assert_eq!(error.status_code(), 404);
        let error = scope
            .admit_write(ScopeOwner::Organization("globex"), &data)
            .unwrap_err();
        assert_eq!(error.status_code(), 403);
        assert!(scope.admit_write(ScopeOwner::Team("core"), &data).is_ok());
    }

    #[test]
    fn the_owner_columns_read_narrowest_first() {
        assert_eq!(
            ScopeOwner::from_columns(None, None, None),
            ScopeOwner::Instance
        );
        assert_eq!(
            ScopeOwner::from_columns(None, None, Some("acme")),
            ScopeOwner::Organization("acme")
        );
        assert_eq!(
            ScopeOwner::from_columns(None, Some("core"), Some("acme")),
            ScopeOwner::Team("core")
        );
        assert_eq!(
            ScopeOwner::from_columns(Some("alice"), Some("core"), Some("acme")),
            ScopeOwner::User("alice")
        );
    }
}
