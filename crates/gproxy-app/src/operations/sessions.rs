//! Console and portal sessions, as an operation rather than as part of the
//! login ladder.
//!
//! Only three things can be done to a session from outside: see that it
//! exists, end it, and end all of a user's. Opening one belongs to
//! authentication (`auth::session`), which is the only place that can mint the
//! token — and the token is the one thing this family never touches. A session
//! row is listed by its id, its user and its two timestamps; `token_hash` has
//! no DTO field and no accessor.
//!
//! None of these is a configuration write. `user_sessions` is not part of
//! `IdentityData` and therefore not part of `AppData`: authentication resolves
//! a session by reading the table on every request, so a revocation takes
//! effect at the next request without any instance reloading anything.
//! Bumping the revision here would make every login and logout invalidate
//! every peer's snapshot for a change no snapshot contains.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::user_session;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use super::{Writer, crud};
use crate::{
    AppError, Result,
    dto::{ListQuery, Page, UserSessionDto},
};

pub struct Sessions<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Sessions<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Sessions<'_, C> {
    /// One user's live and expired sessions.
    ///
    /// Expired rows are included on purpose: a portal showing "where you are
    /// signed in" wants to explain a session that has just ended, and purging
    /// is housekeeping that may not have run.
    pub async fn list(&self, user_id: &str) -> Result<Page<UserSessionDto>> {
        self.page(ListQuery::for_user(user_id)).await
    }

    /// The same, with paging under the caller's control.
    pub async fn page(&self, query: ListQuery) -> Result<Page<UserSessionDto>> {
        let (offset, limit) = query.bounds();
        let mut select = user_session::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(user_session::Column::UserId.eq(user_id));
        }
        let page = self
            .writer
            .store()
            .user_sessions()
            .page(select, offset, limit)
            .await?;
        Ok(Page::convert(page, UserSessionDto::from))
    }

    /// End one session, by its id rather than by its token: an administrator
    /// ending somebody else's session does not have the token and must not
    /// need it.
    pub async fn revoke(&self, id: &str) -> Result<()> {
        let removed = self
            .writer
            .store()
            .user_sessions()
            .delete_many(std::slice::from_ref(&id.to_owned()))
            .await?;
        if removed.first().copied().unwrap_or(0) == 0 {
            return Err(AppError::not_found("session", id));
        }
        Ok(())
    }

    /// End every session a user holds, and answer how many there were.
    ///
    /// This is "sign out everywhere". A password change performs the same
    /// deletion inside its own revision commit, because there it has to be
    /// atomic with the credential it replaces.
    pub async fn revoke_all(&self, user_id: &str) -> Result<u64> {
        let removed = self
            .writer
            .store()
            .user_sessions()
            .delete_where_many(vec![
                user_session::Entity::delete_many()
                    .filter(user_session::Column::UserId.eq(user_id)),
            ])
            .await?;
        Ok(removed.first().copied().unwrap_or(0))
    }

    /// Drop sessions that are already over, and answer how many.
    ///
    /// Housekeeping only: nothing depends on it having run, because
    /// authentication checks expiry itself on every request.
    pub async fn purge_expired(&self, now_ms: i64) -> Result<u64> {
        let removed = self
            .writer
            .store()
            .user_sessions()
            .delete_where_many(vec![
                user_session::Entity::delete_many()
                    .filter(user_session::Column::ExpiresAtMs.lte(now_ms)),
            ])
            .await?;
        Ok(removed.first().copied().unwrap_or(0))
    }
}
