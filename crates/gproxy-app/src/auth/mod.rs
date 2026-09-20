//! Turning a presented credential into a [`Caller`].
//!
//! Three kinds of caller, one type. Everything above this module — admission,
//! the operations, the budget chain — reads a `Caller` and never re-derives
//! who is asking:
//!
//! - a **gateway API key**, hashed and looked up in the snapshot's
//!   [`ApiKeyIndex`](crate::snapshot::ApiKeyIndex). A `kind = OAuth` key is an
//!   OAuth grant's internal key and is refused when presented as a bearer key;
//! - an **OAuth access token**, resolved through the store's
//!   `resolve_access_many`, which checks the grant's liveness, the client, the
//!   user, the key and the subscription in the same statement that reads it;
//! - a **console or portal session**, backed by `user_sessions`, which acts as
//!   the person rather than as a key.
//!
//! Two rules hold across all three.
//!
//! **Tokens are only ever stored as digests.** `api_keys.key_hash`,
//! `user_sessions.token_hash` and `oauth_tokens.token_hash` hold a SHA-256 of
//! the text; the text itself is returned once, at creation, and the instance
//! cannot produce it again. Both hashed-string columns go through
//! [`encode_key_hash`](crate::snapshot::encode_key_hash) so one encoding is
//! used everywhere in this crate.
//!
//! **A failure to authenticate is always `Unauthorized`, never `Forbidden`.**
//! A disabled key, an expired key, a revoked grant and a key that never
//! existed must be indistinguishable from outside; `Forbidden` says "this
//! credential is real, but not entitled", which is exactly the fact an
//! attacker is probing for. `Forbidden` belongs to admission, after identity
//! is settled.

mod api_key;
mod csrf;
mod oauth_access;
pub mod password;
mod session;

pub use api_key::{API_KEY_PREFIX, digests, generate_api_key};
pub use csrf::verify_same_origin;
pub use session::IssuedSession;

use crate::{AppConfig, AppData, AppError, snapshot::Subject};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, entity::identity::user};
use http::HeaderMap;

/// Who is calling, after the credential has been checked and before anything
/// has been decided about what they may do.
///
/// `organization_id`, `team_id` and `subscription_id` come from the API key
/// row (or, for a grant, from the grant's internal key row) and never from a
/// request header. That single binding decides the budget owner chain, the
/// permission subject and the credential-visibility boundary at once, so a
/// client that could choose it could choose who pays and what it can see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    pub user_id: String,
    /// The instance-wide role from `users.role`. Organization and team roles
    /// are memberships, not this.
    pub user_role: String,
    /// None for a session caller: a person is not a key.
    pub api_key_id: Option<String>,
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub subscription_id: Option<String>,
    /// Set only for [`CallerKind::OAuthGrant`], and what the issuer's scope
    /// policy is evaluated against.
    pub grant: Option<GrantContext>,
    pub kind: CallerKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerKind {
    ApiKey,
    OAuthGrant,
    Session,
}

/// The grant behind an OAuth access token.
///
/// `access_digest` is the digest the token was resolved by, kept so a later
/// stage can re-resolve the same token — admission re-reads the grant before
/// an expensive call — without the plaintext travelling any further.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrantContext {
    pub grant_id: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub access_digest: [u8; 32],
}

impl Caller {
    /// Instance administrator, which is the only role that is not a
    /// membership. Organization and team administration is answered by
    /// [`MembershipIndex`](crate::snapshot::MembershipIndex).
    pub fn is_instance_admin(&self) -> bool {
        self.user_role == "admin"
    }

    /// The subject [`PermissionSet::decide`](crate::snapshot::PermissionSet::decide)
    /// evaluates. A key is its own subject *and* its user's; a session caller
    /// has no key, so key-scoped rules never apply to it.
    pub fn subject(&self) -> Subject<'_> {
        Subject {
            user_id: &self.user_id,
            api_key_id: self.api_key_id.as_deref(),
        }
    }

    fn from_key(identity: &crate::snapshot::ApiKeyIdentity) -> Self {
        Self {
            user_id: identity.user_id.clone(),
            user_role: identity.user_role.clone(),
            api_key_id: Some(identity.api_key_id.clone()),
            organization_id: identity.organization_id.clone(),
            team_id: identity.team_id.clone(),
            subscription_id: identity.subscription_id.clone(),
            grant: None,
            kind: CallerKind::ApiKey,
        }
    }
}

/// Authentication against one snapshot and one database.
///
/// Borrowed rather than owned: a request already holds the `Arc<AppData>` it
/// decided under, and the whole point of the snapshot is that the same value
/// answers every question in that request.
pub struct Authenticator<'a, C> {
    store: &'a Store<C>,
    snapshot: &'a AppData,
    config: &'a AppConfig,
}

impl<'a, C> Authenticator<'a, C> {
    pub fn new(store: &'a Store<C>, snapshot: &'a AppData, config: &'a AppConfig) -> Self {
        Self {
            store,
            snapshot,
            config,
        }
    }

    pub fn snapshot(&self) -> &'a AppData {
        self.snapshot
    }

    pub fn config(&self) -> &'a AppConfig {
        self.config
    }

    pub fn store(&self) -> &'a Store<C> {
        self.store
    }
}

impl<C: BatchConnectionTrait> Authenticator<'_, C> {
    /// The data-plane ladder over headers alone: `Authorization: Bearer …`,
    /// then `x-api-key`, then `x-goog-api-key`.
    ///
    /// A key in the query string (`?key=`, which the Gemini dialect uses) is
    /// deliberately not read here. Query strings belong to the transport: the
    /// host has already parsed the URI, this crate has not, and re-parsing it
    /// from a header map is guesswork. A host that accepts one extracts it and
    /// calls [`Authenticator::authenticate_token`].
    pub async fn authenticate_request(&self, headers: &HeaderMap) -> Result<Caller, AppError> {
        let token = bearer_token(headers).ok_or(AppError::Unauthorized("no credential present"))?;
        self.authenticate_token(token).await
    }

    /// The full ladder for one presented token, against the current clock.
    pub async fn authenticate_token(&self, token: &str) -> Result<Caller, AppError> {
        self.authenticate_token_at(token, crate::now_ms()).await
    }

    /// [`Authenticator::authenticate_token`] against a stated clock, for tests
    /// that need to sit on an expiry boundary.
    ///
    /// The order is: gateway API key first, OAuth access token second. A key
    /// hit is decided there and then — including a refusal — and never falls
    /// through to the OAuth path, because an OAuth grant's internal key is
    /// exactly the credential that must not be usable as a bearer key.
    pub async fn authenticate_token_at(
        &self,
        token: &str,
        now_ms: i64,
    ) -> Result<Caller, AppError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AppError::Unauthorized("empty credential"));
        }
        if let Some(caller) = self.authenticate_api_key(token, now_ms)? {
            return Ok(caller);
        }
        if let Some(caller) = self.authenticate_access_token(token, now_ms).await? {
            return Ok(caller);
        }
        Err(AppError::Unauthorized("credential matches nothing"))
    }

    /// The instance role of a user who is allowed to authenticate, or None
    /// when the user is unknown or disabled.
    ///
    /// The snapshot answers first. It can legitimately be behind — a user
    /// created by the operation that is now logging them in is one revision
    /// newer than the snapshot the request loaded — so a miss falls back to a
    /// read rather than refusing a valid credential.
    async fn user_role(&self, user_id: &str) -> Result<Option<String>, AppError> {
        if let Some(user) = self.snapshot.users.get(user_id) {
            return Ok(user.enabled.then(|| user.role.clone()));
        }
        let row = self
            .store
            .users()
            .get_many(&[user_id.to_string()])
            .await?
            .into_iter()
            .next()
            .flatten();
        Ok(row
            .filter(|row: &user::Model| row.enabled)
            .map(|row| row.role))
    }
}

/// The token a data-plane request presents, in the order the dialects put it.
/// Empty values are treated as absent: a client that sends `x-api-key:` with
/// nothing after it has presented no credential, not a credential of length
/// zero.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let bearer = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(strip_bearer);
    bearer
        .or_else(|| headers.get("x-api-key")?.to_str().ok())
        .or_else(|| headers.get("x-goog-api-key")?.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// `Bearer` is case-insensitive per RFC 6750; clients spell it every way.
fn strip_bearer(value: &str) -> Option<&str> {
    let (scheme, rest) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn every_dialects_header_is_read_in_order() {
        assert_eq!(
            bearer_token(&headers(&[("authorization", "Bearer sk-one")])),
            Some("sk-one")
        );
        assert_eq!(
            bearer_token(&headers(&[("authorization", "bearer sk-one")])),
            Some("sk-one")
        );
        assert_eq!(
            bearer_token(&headers(&[("x-api-key", "sk-two")])),
            Some("sk-two")
        );
        assert_eq!(
            bearer_token(&headers(&[("x-goog-api-key", "sk-three")])),
            Some("sk-three")
        );
        // Authorization wins when several are present.
        assert_eq!(
            bearer_token(&headers(&[
                ("authorization", "Bearer sk-one"),
                ("x-api-key", "sk-two"),
            ])),
            Some("sk-one")
        );
    }

    #[test]
    fn an_absent_blank_or_foreign_scheme_is_no_credential() {
        assert_eq!(bearer_token(&headers(&[])), None);
        assert_eq!(bearer_token(&headers(&[("x-api-key", "   ")])), None);
        assert_eq!(
            bearer_token(&headers(&[("authorization", "Bearer ")])),
            None
        );
        assert_eq!(
            bearer_token(&headers(&[("authorization", "Basic abc")])),
            None
        );
        assert_eq!(bearer_token(&headers(&[("authorization", "sk-raw")])), None);
    }

    #[test]
    fn a_caller_reports_its_role_and_permission_subject() {
        let mut caller = Caller {
            user_id: "u1".into(),
            user_role: "user".into(),
            api_key_id: Some("k1".into()),
            organization_id: None,
            team_id: None,
            subscription_id: None,
            grant: None,
            kind: CallerKind::ApiKey,
        };
        assert!(!caller.is_instance_admin());
        assert_eq!(caller.subject().user_id, "u1");
        assert_eq!(caller.subject().api_key_id, Some("k1"));

        caller.user_role = "admin".into();
        caller.api_key_id = None;
        assert!(caller.is_instance_admin());
        // A session caller carries no key, so key-scoped rules cannot apply.
        assert_eq!(caller.subject().api_key_id, None);
    }
}
