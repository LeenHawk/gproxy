//! The self-serve surface: what a user may see and change **about
//! themselves**.
//!
//! The admin families in the parent module are an operator's tools — they take
//! an id and act on whatever row it names. This one is the other half of the
//! product: the person who holds a key, signing in to see their own keys,
//! their own spend, the models they may call and the programs they have
//! authorized.
//!
//! # Scoping is by construction, not by checking
//!
//! [`Portal`] is built from a [`Caller`] and **no method takes a user id**.
//! There is no parameter in this module through which a caller could name
//! somebody else's user, and the one request field that looks like one —
//! [`PortalUsageQuery::user_id`](crate::dto::PortalUsageQuery::user_id) — is
//! overwritten with the caller's own id
//! before the query runs rather than validated against it. A filter that is
//! overwritten cannot be forgotten; a filter that is checked can.
//!
//! Where an id *is* unavoidable — a key id, a grant id — the row is read and
//! its owner compared to the caller before anything happens, and a row that
//! belongs to somebody else answers **`NotFound`, never `Forbidden`**.
//! `Forbidden` would confirm that the id exists, which is exactly the fact an
//! enumeration is looking for; from outside, another user's key and an id that
//! was never minted must be the same answer.
//!
//! # It reuses the admin families rather than repeating them
//!
//! A portal key create is `api_keys().create` with the caller's own user id
//! filled in, so the binding rules (the organization exists, the caller is a
//! member of it, a team's parent is the named organization) are validated once
//! in one place. A portal password change is `users().set_password` after the
//! current password has been proved. What this module adds is the scoping and
//! the read-side composition — never a second copy of a rule.
//!
//! # What the portal deliberately sees less of
//!
//! Compared with the operator's log view, [`Portal::recent_requests`] returns
//! no bodies, no headers, no URL, no client address, no credential id and no
//! provider id. A captured body can contain the caller's own prompt, which is
//! theirs — but it can equally contain a system prompt, a tool definition or
//! an upstream error that belongs to the operator, and a credential or
//! provider id is infrastructure the account holder has no use for and an
//! attacker does. The provider survives as a display name because "which
//! upstream served this" is a fair question; everything that identifies the
//! machinery does not. The list is also gated by
//! `settings.portal_recent_requests_enabled`, and this module is that column's
//! only consumer.
//!
//! # Signing in is not here
//!
//! [`Operations::portal_login`] lives on [`Operations`] rather than on
//! [`Portal`], for the one reason that matters: it precedes the caller. A type
//! whose entire contract is "every method is scoped to this caller" cannot
//! also hold the method that runs before there is one. [`Portal::logout`] *is*
//! here, because by then there is.
//!
//! Rate limiting of sign-in attempts is the **host's**. This crate has no
//! client address — it never parses a forwarding header — and a limit keyed on
//! anything else would either lock one username out globally or not limit
//! anything at all.

mod keys;
mod models;
mod oauth_sessions;
mod password;
mod usage;

pub use keys::PortalKeys;
pub use models::MAX_PORTAL_MODELS;
pub use oauth_sessions::PortalOAuthSessions;
pub use password::PortalPassword;
pub use usage::MAX_RECENT_REQUESTS;

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::{membership_role::MembershipRole, user};

use super::{Operations, Writer};
use crate::{
    AppConfig, AppData, AppError, Caller, CallerKind, Result,
    auth::{Authenticator, IssuedSession, password as password_policy},
    dto::{
        PortalContextDto, PortalFeaturesDto, PortalOrganizationDto, PortalSubscriptionDto,
        PortalTeamDto, PortalUserDto, UserSessionDto,
    },
};

/// The portal operations of one caller, over one handle, one snapshot and one
/// configuration.
///
/// The caller is held by value rather than by reference so this type carries a
/// single lifetime; a `Caller` is a handful of short strings and an accessor is
/// built once per operation, not once per row.
pub struct Portal<'a, C> {
    writer: Writer<'a, C>,
    config: &'a AppConfig,
    caller: Caller,
}

impl<'a, C> Portal<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>, config: &'a AppConfig, caller: Caller) -> Self {
        Self {
            writer,
            config,
            caller,
        }
    }

    /// Who this surface is scoped to. Read-only: nothing here can be pointed
    /// at somebody else after construction.
    pub fn caller(&self) -> &Caller {
        &self.caller
    }

    pub(crate) fn data(&self) -> &'a AppData {
        self.writer.data()
    }

    pub(crate) fn config(&self) -> &'a AppConfig {
        self.config
    }

    pub(crate) fn writer(&self) -> Writer<'a, C> {
        self.writer
    }

    /// The admin families, so a portal operation can delegate to the one that
    /// already owns a rule instead of restating it.
    pub(crate) fn operations(&self) -> Operations<'a, C> {
        Operations::new(self.writer.gproxy(), self.writer.data(), self.config)
    }

    /// The caller's own gateway keys.
    pub fn keys(&self) -> PortalKeys<'a, C> {
        PortalKeys::new(self.writer, self.config, self.caller.clone())
    }

    /// The caller's own OAuth authorizations.
    pub fn oauth_sessions(&self) -> PortalOAuthSessions<'a, C> {
        PortalOAuthSessions::new(self.writer, self.config, self.caller.clone())
    }

    /// The caller's own password.
    pub fn password(&self) -> PortalPassword<'a, C> {
        PortalPassword::new(self.writer, self.config, self.caller.clone())
    }

    /// Refuse an operation that an OAuth-grant caller must not perform on the
    /// account that authorized it.
    ///
    /// `Forbidden` and not `NotFound` on purpose: this names no row. It is a
    /// policy about the *kind* of credential in hand, and the caller can act
    /// on being told — by signing in to the portal directly.
    pub(crate) fn require_person(&self, what: &str) -> Result<()> {
        if self.caller.kind == CallerKind::OAuthGrant {
            return Err(AppError::forbidden(format!(
                "an OAuth client may not {what}; sign in to the portal instead"
            )));
        }
        Ok(())
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Portal<'_, C> {
    /// Everything a portal loads first: the caller, their scopes, their
    /// subscription and what this instance offers them.
    ///
    /// One database read — the settings row, for the recent-requests switch —
    /// and the rest from the snapshot the request already pinned.
    pub async fn context(&self) -> Result<PortalContextDto> {
        let user = self.user().await?;
        let data = self.data();
        let memberships = &data.memberships;

        let organizations = memberships
            .organizations_of(&self.caller.user_id)
            .iter()
            .map(|(id, role)| PortalOrganizationDto {
                name: data
                    .organizations
                    .get(id)
                    .map_or_else(|| id.clone(), |row| row.name.clone()),
                id: id.clone(),
                role: role_name(*role).to_owned(),
            })
            .collect();
        let teams = memberships
            .teams_of(&self.caller.user_id)
            .iter()
            .filter_map(|(id, role)| {
                // A team whose row is gone from the snapshot has no parent to
                // report, and a team without an organization is not a scope a
                // portal can render.
                let team = data.teams.get(id)?;
                Some(PortalTeamDto {
                    id: id.clone(),
                    name: team.name.clone(),
                    organization_id: team.organization_id.clone(),
                    role: role_name(*role).to_owned(),
                })
            })
            .collect();

        Ok(PortalContextDto {
            user: PortalUserDto {
                id: user.id,
                name: user.name,
                role: user.role,
                has_password: user.password_hash.is_some(),
            },
            organizations,
            teams,
            subscription: self.subscription(),
            features: self.features().await?,
        })
    }

    /// The caller's own user row.
    ///
    /// Read from the database rather than from the snapshot: this is what a
    /// portal renders its own account page from, and a snapshot that is one
    /// revision behind would show a name or a password state the user has just
    /// changed as unchanged.
    async fn user(&self) -> Result<user::Model> {
        self.writer
            .store()
            .users()
            .get_many(std::slice::from_ref(&self.caller.user_id))
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| AppError::not_found("user", &self.caller.user_id))
    }

    /// The subscription in force: the one the calling key selected, or — when
    /// it selected none — the caller's own enabled subscription.
    ///
    /// Never somebody else's: the key-selected id was validated as the key
    /// holder's when the key was bound, and the fallback is filtered by user
    /// id here.
    fn subscription(&self) -> Option<PortalSubscriptionDto> {
        let data = self.data();
        let row = match &self.caller.subscription_id {
            Some(id) => data
                .subscriptions
                .get(id)
                .filter(|row| row.user_id == self.caller.user_id)?,
            None => data
                .subscriptions
                .values()
                .filter(|row| row.user_id == self.caller.user_id && row.enabled)
                // Deterministic when a user holds several: the newest wins,
                // and the id breaks a tie within one millisecond.
                .max_by(|a, b| (a.created_at_ms, &a.id).cmp(&(b.created_at_ms, &b.id)))?,
        };
        let plan_name = data.plans.get(&row.plan_id).map(|plan| plan.name.clone());
        Some(PortalSubscriptionDto::new(row, plan_name))
    }

    /// The feature flags, from configuration and the caller's role. Nothing
    /// here reads a row the caller could not read anyway.
    async fn features(&self) -> Result<PortalFeaturesDto> {
        let person = self.caller.kind != CallerKind::OAuthGrant;
        Ok(PortalFeaturesDto {
            can_create_keys: person,
            can_change_password: person,
            can_see_logs: self.recent_requests_enabled().await?,
            can_see_console: self.config.console.enabled && self.caller.is_instance_admin(),
        })
    }

    /// End the session this request arrived on. Answers whether a row was
    /// removed, so a logout can be audited as a real one rather than a repeat.
    ///
    /// This one names no row and needs no ownership check: the token *is* the
    /// proof, and a caller who holds it is the session. Ending somebody else's
    /// session would mean holding their token, which is a compromise this
    /// check could not have prevented anyway.
    pub async fn logout(&self, token: &str) -> Result<bool> {
        Authenticator::new(self.writer.store(), self.data(), self.config)
            .revoke_session(token)
            .await
    }

    /// Where the caller is signed in. Scoped by user id through the admin
    /// family, which already refuses to report a token or a digest.
    ///
    /// Expired rows are included: a portal explaining "this session ended
    /// yesterday" is more useful than one that silently drops it, and purging
    /// is housekeeping that may not have run.
    pub async fn sessions(&self) -> Result<Vec<UserSessionDto>> {
        Ok(self
            .operations()
            .sessions()
            .list(&self.caller.user_id)
            .await?
            .items)
    }
}

fn role_name(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::Member => "member",
        MembershipRole::Admin => "admin",
    }
}

impl<'a, C> Operations<'a, C> {
    /// The self-serve surface for one caller. Every operation on the returned
    /// value is scoped to them, and none of them takes a user id.
    pub fn portal(&self, caller: &Caller) -> Portal<'a, C> {
        Portal::new(self.writer(), self.config(), caller.clone())
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Operations<'_, C> {
    /// Open a portal session from a name and a password.
    ///
    /// Not on [`Portal`] because it runs before there is a caller. An unknown
    /// name, a disabled account, an account with no password and a wrong
    /// password are one indistinguishable `Unauthorized`: telling them apart
    /// turns the sign-in form into an account-enumeration oracle.
    ///
    /// Every attempt costs one argon2 verification, which is why **the host
    /// rate-limits this**; this crate has no client address to key a limit on.
    pub async fn portal_login(&self, name: &str, password: &str) -> Result<IssuedSession> {
        self.portal_login_at(name, password, crate::now_ms()).await
    }

    /// [`Operations::portal_login`] against a stated clock, so a test can sit
    /// on a session's expiry boundary.
    pub async fn portal_login_at(
        &self,
        name: &str,
        password: &str,
        now_ms: i64,
    ) -> Result<IssuedSession> {
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

        /// One answer for every way a sign-in can fail.
        fn refused() -> AppError {
            AppError::Unauthorized("portal sign-in refused")
        }

        let name = name.trim();
        if name.is_empty() || password.is_empty() {
            return Err(refused());
        }
        let row = self
            .store()
            .users()
            .query(user::Entity::find().filter(user::Column::Name.eq(name)))
            .await?
            .into_iter()
            .next()
            .filter(|row| row.enabled)
            .ok_or_else(refused)?;
        let stored = row.password_hash.as_deref().ok_or_else(refused)?;
        if !password_policy::verify(password, stored) {
            return Err(refused());
        }
        self.authenticator().create_session(&row.id, now_ms).await
    }

    /// End a session by the token that opened it, without needing a caller —
    /// a client whose session has already expired still wants its cookie
    /// cleared. Answers whether a row was actually removed.
    pub async fn portal_logout(&self, token: &str) -> Result<bool> {
        self.authenticator().revoke_session(token).await
    }

    fn authenticator(&self) -> Authenticator<'_, C> {
        Authenticator::new(self.store(), self.data(), self.config())
    }
}
