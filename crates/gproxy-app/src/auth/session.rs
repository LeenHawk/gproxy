//! Console and portal sessions, backed by `user_sessions`.
//!
//! A session is the third caller kind and the only one that is not a key: it
//! acts as the person. It therefore carries no `api_key_id` and no
//! organization or team binding. That is not an omission — a key
//! binding is a fixed scope chosen when the key was created, while a console
//! user works across every organization they are a member of, and which one an
//! operation touches is an argument of that operation, checked against
//! [`MembershipIndex`](crate::snapshot::MembershipIndex). Deriving a binding
//! here would silently pin a console session to one organization.
//!
//! Session tokens are stored exactly like key digests — the lowercase hex of
//! their SHA-256, through
//! [`encode_key_hash`](crate::snapshot::encode_key_hash) — so every hashed
//! string column this crate writes shares one encoding.

use super::{Authenticator, Caller, CallerKind, api_key::random_bytes};
use crate::{AppError, snapshot::encode_key_hash};
use base64::Engine;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::user_session;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use sha2::{Digest, Sha256};

/// A freshly created session. `token` is the only time the plaintext exists;
/// the row holds its digest, and a lost token is replaced, never recovered.
#[derive(Clone, Debug)]
pub struct IssuedSession {
    pub id: String,
    pub token: String,
    pub expires_at_ms: i64,
}

impl<C: BatchConnectionTrait> Authenticator<'_, C> {
    /// Open a session for a user who has already been authenticated by some
    /// other means — a password, or a first-run bootstrap.
    ///
    /// This does not check the password and does not check the user: it is the
    /// step *after* that decision, and a caller that reaches it has made one.
    pub async fn create_session(
        &self,
        user_id: &str,
        now_ms: i64,
    ) -> Result<IssuedSession, AppError> {
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
        let id = crate::hex::encode(&random_bytes::<16>()?);
        let ttl_ms = i64::try_from(self.config().session_ttl_secs)
            .unwrap_or(i64::MAX)
            .saturating_mul(1_000);
        let expires_at_ms = now_ms.saturating_add(ttl_ms);
        self.store()
            .user_sessions()
            .create_many(vec![user_session::ActiveModel {
                id: Set(id.clone()),
                user_id: Set(user_id.to_string()),
                token_hash: Set(token_hash(&token)),
                created_at_ms: Set(now_ms),
                expires_at_ms: Set(expires_at_ms),
            }])
            .await?;
        Ok(IssuedSession {
            id,
            token,
            expires_at_ms,
        })
    }

    /// Resolve a session cookie to its caller.
    ///
    /// Expiry is checked here rather than left to [`Self::purge_expired_sessions`]:
    /// purging is housekeeping that may not have run, and a session that is
    /// past its end must stop working the instant it is, not the next time
    /// something sweeps the table.
    pub async fn authenticate_session(&self, token: &str, now_ms: i64) -> Result<Caller, AppError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::Unauthorized("empty session token"));
        }
        let row = self
            .store()
            .user_sessions()
            .query(
                user_session::Entity::find()
                    .filter(user_session::Column::TokenHash.eq(token_hash(token))),
            )
            .await?
            .into_iter()
            .next()
            .ok_or(AppError::Unauthorized("session token matches nothing"))?;
        if row.expires_at_ms <= now_ms {
            return Err(AppError::Unauthorized("session is expired"));
        }
        let user_role = self
            .user_role(&row.user_id)
            .await?
            .ok_or(AppError::Unauthorized("session user is disabled or gone"))?;
        Ok(Caller {
            user_id: row.user_id,
            user_role,
            api_key_id: None,
            organization_id: None,
            team_id: None,
            grant: None,
            kind: CallerKind::Session,
        })
    }

    /// End one session. Answers whether a row was actually removed, so a
    /// logout can be audited as a real one rather than a repeat.
    pub async fn revoke_session(&self, token: &str) -> Result<bool, AppError> {
        let removed = self
            .store()
            .user_sessions()
            .delete_where_many(vec![
                user_session::Entity::delete_many()
                    .filter(user_session::Column::TokenHash.eq(token_hash(token.trim()))),
            ])
            .await?;
        Ok(removed.first().copied().unwrap_or(0) > 0)
    }

    /// End every session a user holds, and answer how many. This is what a
    /// password change, a disable and an administrative "sign out everywhere"
    /// run: a credential that is no longer valid must not leave live sessions
    /// behind it.
    pub async fn revoke_all_sessions_for_user(&self, user_id: &str) -> Result<u64, AppError> {
        let removed = self
            .store()
            .user_sessions()
            .delete_where_many(vec![
                user_session::Entity::delete_many()
                    .filter(user_session::Column::UserId.eq(user_id)),
            ])
            .await?;
        Ok(removed.first().copied().unwrap_or(0))
    }

    /// Drop sessions that are already over, and answer how many. Housekeeping
    /// only — nothing depends on it having run, because authentication checks
    /// expiry itself.
    pub async fn purge_expired_sessions(&self, now_ms: i64) -> Result<u64, AppError> {
        let removed = self
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

/// The stored form of a session token.
fn token_hash(token: &str) -> String {
    encode_key_hash(&Sha256::digest(token.as_bytes()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_token_is_stored_under_the_same_encoding_as_a_key() {
        let hash = token_hash("abc");
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash.to_lowercase());
        assert_eq!(
            crate::snapshot::decode_key_hash(&hash).unwrap(),
            <[u8; 32]>::from(Sha256::digest(b"abc"))
        );
    }
}
