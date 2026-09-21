//! The programs the caller has authorized.
//!
//! An OAuth grant is a user's decision to let somebody else's binary act as
//! them. The portal is where that decision is reviewed and withdrawn, which
//! makes this the one place a user can see what they have handed out.
//!
//! Listing is scoped in SQL by the caller's own user id. Revoking takes an id,
//! so it reads the row first and answers `NotFound` for one that is not the
//! caller's — same rule and same reason as the key family: a revoke that
//! answered `Forbidden` would confirm the grant exists.
//!
//! # Revocation is two writes, in this order
//!
//! The store's `revoke_many` is itself atomic: it marks the grant revoked,
//! disables the grant's internal API key and revokes every issued token
//! together. A second, empty revision commit follows, whose only job is to
//! move `config_revision` and notify the peers — the disabled key is an
//! `api_keys` row and therefore part of [`AppData`](crate::AppData).
//!
//! If the second fails, the grant is still revoked and the peers converge on
//! the revision poll. The window that leaves is one in which a peer's snapshot
//! is *stricter* than nothing and looser than the database — and it is
//! harmless either way, because an `oauth` key is refused as a bearer
//! credential regardless, and an access token is resolved by a database read
//! that checks the grant's liveness on every request. Nothing serves traffic
//! from the stale row.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::oauth::grant;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};

use super::Portal;
use crate::{
    AppConfig, AppError, Caller, Result,
    dto::PortalOAuthSessionDto,
    operations::{Scope, Writer},
};

pub struct PortalOAuthSessions<'a, C> {
    portal: Portal<'a, C>,
}

impl<'a, C> PortalOAuthSessions<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>, config: &'a AppConfig, caller: Caller) -> Self {
        Self {
            portal: Portal::new(writer, config, caller),
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PortalOAuthSessions<'_, C> {
    /// The caller's live authorizations, newest first.
    ///
    /// Revoked grants are not listed. A key or a session is listed after it
    /// stops working because the user may need to be told *why* it stopped;
    /// a revoked grant stopped because this same screen revoked it, and
    /// keeping it would turn the list into a history nobody asked for.
    pub async fn list(&self) -> Result<Vec<PortalOAuthSessionDto>> {
        let rows = self
            .portal
            .writer()
            .store()
            .oauth_grants()
            .query(
                grant::Entity::find()
                    .filter(grant::Column::UserId.eq(&self.portal.caller().user_id))
                    .filter(grant::Column::RevokedAtMs.is_null())
                    .order_by_desc(grant::Column::CreatedAtMs)
                    .order_by_asc(grant::Column::Id),
            )
            .await?;
        let clients = self.portal.data();
        Ok(rows
            .into_iter()
            .map(|row| {
                let name = clients
                    .oauth_clients
                    .get(&row.client_id)
                    .map(|client| client.name.clone());
                PortalOAuthSessionDto::new(row, name)
            })
            .collect())
    }

    /// Withdraw one of the caller's own authorizations.
    ///
    /// Idempotent at the store level, but not here: a grant that is already
    /// revoked is no longer one of the caller's live grants and answers
    /// `NotFound`, the same as a grant that was never theirs. A portal that
    /// double-clicks the button sees the same thing either way, which is what
    /// it should.
    pub async fn revoke(&self, grant_id: &str) -> Result<()> {
        self.own(grant_id).await?;
        let revoked = self
            .portal
            .writer()
            .store()
            .oauth_grants()
            .revoke_many(std::slice::from_ref(&grant_id.to_owned()), crate::now_ms())
            .await?;
        if !revoked.first().copied().unwrap_or(false) {
            // Lost a race with a concurrent revocation. The grant is gone
            // either way, and the caller asked for exactly that.
            return Err(AppError::not_found("oauth session", grant_id));
        }
        // The grant's internal key is now disabled, and that row is in the
        // identity snapshot: tell the peers.
        self.portal
            .writer()
            .commit(Vec::new(), &[Scope::Keys, Scope::OAuthClients])
            .await?;
        Ok(())
    }

    /// That `grant_id` names a live grant of the caller's.
    ///
    /// From the database, not the snapshot: grants are not in
    /// [`AppData`](crate::AppData) at all, and a revocation must be decided
    /// against the row as it is now.
    async fn own(&self, grant_id: &str) -> Result<grant::Model> {
        self.portal
            .writer()
            .store()
            .oauth_grants()
            .get_many(&[grant_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|row| {
                row.user_id == self.portal.caller().user_id && row.revoked_at_ms.is_none()
            })
            .ok_or_else(|| AppError::not_found("oauth session", grant_id))
    }
}
