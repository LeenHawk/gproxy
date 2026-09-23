//! The OAuth access-token half of the ladder.
//!
//! An access token is not a gateway key and is not hashed like one: it is
//! stored in `oauth_tokens.token_hash` as raw bytes, under the digest of the
//! token exactly as presented. No presentation prefix is stripped — the issuer
//! mints these, nothing reshapes them for a channel, and stripping would only
//! widen what a token can match.
//!
//! The liveness of a grant is not re-implemented here. `resolve_access_many`
//! resolves the token and the whole chain behind it in one statement: the
//! token is an access token, unexpired, unrevoked; the grant is unrevoked; the
//! client is enabled and not soft-deleted; the user is enabled; the internal
//! key is enabled, unexpired and of `kind = OAuth`; and the client allowlist
//! admits the pairing. A check done in
//! the same statement as the read cannot be raced by a concurrent revocation.

use super::{Authenticator, Caller, CallerKind, GrantContext};
use crate::AppError;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::identity::api_key;
use sha2::{Digest, Sha256};

impl<C: BatchConnectionTrait> Authenticator<'_, C> {
    /// Step (c) of the ladder. `Ok(None)` when no live grant holds this token,
    /// which is a miss rather than a refusal: the caller turns it into the
    /// single `Unauthorized` at the bottom of the ladder.
    pub async fn authenticate_access_token(
        &self,
        token: &str,
        now_ms: i64,
    ) -> Result<Option<Caller>, AppError> {
        let access_digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let Some(access) = self
            .store()
            .oauth_grants()
            .resolve_access_many(&[access_digest.to_vec()], now_ms)
            .await?
            .into_iter()
            .next()
            .flatten()
        else {
            return Ok(None);
        };
        let grant = access.grant;
        // The grant proves the user is enabled; the role is what the snapshot
        // knows about them. A user this snapshot has never seen gets the least
        // privileged role rather than a refusal or a guess upwards.
        let user_role = self
            .user_role(&grant.user_id)
            .await?
            .unwrap_or_else(|| "user".into());
        // The binding lives on the grant's internal key, not on the grant, and
        // it is what decides who pays and which credentials are visible.
        let key = self.grant_key(&grant.api_key_id).await?;
        Ok(Some(Caller {
            user_id: grant.user_id.clone(),
            user_role,
            api_key_id: Some(grant.api_key_id.clone()),
            organization_id: key.as_ref().and_then(|row| row.organization_id.clone()),
            team_id: key.as_ref().and_then(|row| row.team_id.clone()),
            grant: Some(GrantContext {
                grant_id: grant.id,
                client_id: grant.client_id,
                scopes: scopes(&grant.scopes),
                access_digest,
            }),
            kind: CallerKind::OAuthGrant,
        }))
    }

    /// The grant's internal key binding: organization and team.
    ///
    /// The snapshot answers when it can. It often cannot: an OAuth key is
    /// created by the authorization that issued the grant, so a token redeemed
    /// immediately afterwards can be one revision ahead of the snapshot this
    /// request loaded. `resolve_access_many` has already proven the key live,
    /// so a miss reads the row instead of dropping the binding — losing it
    /// silently would bill the wrong owner and widen what the grant can see.
    async fn grant_key(&self, api_key_id: &str) -> Result<Option<Binding>, AppError> {
        if let Some(identity) = self.snapshot().keys.lookup_id(api_key_id) {
            return Ok(Some(Binding {
                organization_id: identity.organization_id.clone(),
                team_id: identity.team_id.clone(),
            }));
        }
        let row = self
            .store()
            .api_keys()
            .get_many(&[api_key_id.to_string()])
            .await?
            .into_iter()
            .next()
            .flatten();
        Ok(row.map(|row: api_key::Model| Binding {
            organization_id: row.organization_id,
            team_id: row.team_id,
        }))
    }
}

struct Binding {
    organization_id: Option<String>,
    team_id: Option<String>,
}

/// `oauth_grants.scopes` is JSON. The issuer writes an array of strings; a
/// space-delimited string is accepted too because that is the wire form in
/// every OAuth request, and a row written from one is not worth refusing a
/// live grant over. Anything else yields no scopes, which grants nothing.
fn scopes(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(str::to_string)
            .collect(),
        serde_json::Value::String(text) => text.split_whitespace().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scopes_read_an_array_or_the_wire_string() {
        assert_eq!(scopes(&json!(["openid", "profile"])), ["openid", "profile"]);
        assert_eq!(scopes(&json!("openid  profile")), ["openid", "profile"]);
    }

    #[test]
    fn a_shape_that_is_not_a_scope_list_grants_nothing() {
        assert!(scopes(&json!(null)).is_empty());
        assert!(scopes(&json!(7)).is_empty());
        assert!(scopes(&json!({"openid": true})).is_empty());
        assert_eq!(scopes(&json!(["openid", 7, null])), ["openid"]);
    }
}
