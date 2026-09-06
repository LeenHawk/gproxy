use crate::backend::Row;
use crate::query::oauth_sessions;
use crate::records::{OAuthSessionPage, OAuthSessionRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn oauth_sessions(
        &self,
        user_id: i64,
        now: i64,
        active_only: bool,
        limit: u64,
        offset: u64,
    ) -> Result<OAuthSessionPage, StoreError> {
        let mut results = self
            .backend()
            .batch(vec![
                oauth_sessions::summary(user_id, now)?,
                oauth_sessions::list(user_id, now, active_only, limit, offset)?,
            ])
            .await?
            .into_iter();
        let summary = results
            .next()
            .expect("summary result")
            .rows
            .into_iter()
            .next()
            .expect("aggregate row");
        let total_logins = summary.i64("total_logins")?;
        let active_sessions = summary.optional_i64("active_sessions")?.unwrap_or(0);
        Ok(OAuthSessionPage {
            sessions: results
                .next()
                .expect("session result")
                .rows
                .into_iter()
                .map(parse)
                .collect::<Result<_, _>>()?,
            total_logins,
            active_sessions,
            total: if active_only {
                active_sessions
            } else {
                total_logins
            },
        })
    }

    pub async fn revoke_owned_oauth_session(
        &self,
        user_id: i64,
        id: i64,
        now: i64,
    ) -> Result<bool, StoreError> {
        let result = self
            .backend()
            .execute(oauth_sessions::owned_key(user_id, id)?)
            .await?;
        let Some(row) = result.rows.first() else {
            return Ok(false);
        };
        self.revoke_oauth_grant(id, row.i64("user_key_id")?, now)
            .await?;
        Ok(true)
    }

    pub async fn is_oauth_user_key(&self, id: i64) -> Result<bool, StoreError> {
        Ok(!self
            .backend()
            .execute(oauth_sessions::internal_key(id)?)
            .await?
            .rows
            .is_empty())
    }

    pub async fn oauth_user_key_ids(&self) -> Result<Vec<i64>, StoreError> {
        let statement = crate::backend::Statement::query(&oauth_sessions::internal_keys())?;
        self.backend()
            .execute(statement)
            .await?
            .rows
            .iter()
            .map(|row| row.i64("user_key_id"))
            .collect()
    }
}

fn parse(row: Row) -> Result<OAuthSessionRecord, StoreError> {
    Ok(OAuthSessionRecord {
        id: row.i64("id")?,
        client_id: row.text("client_id")?.into(),
        client_name: row.text("client_name")?.into(),
        logged_in_at: row.i64("logged_in_at")?,
        last_refreshed_at: row.optional_i64("last_refreshed_at")?,
        refresh_count: row.optional_i64("refresh_count")?,
        refresh_expires_at: row.optional_i64("refresh_expires_at")?,
        revoked_at: row.optional_i64("revoked_at")?,
        active: row.i64("active")? != 0,
    })
}
