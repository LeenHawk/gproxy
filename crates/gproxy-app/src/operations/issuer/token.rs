//! The token and revocation endpoints.
//!
//! Three grant types reach [`Issuer::token`], and all three end in the same
//! place: `exchange_tokens_many`, which consumes the one-shot credential and
//! inserts the replacement pair in a single atomic batch. Nothing here
//! implements "check, then write" — the check *is* the write's `WHERE` clause,
//! so two redemptions of the same code cannot both succeed however they are
//! interleaved.
//!
//! # Replay, and why it costs the whole grant
//!
//! A code and a refresh token are single-use. When one is presented twice,
//! there are two possibilities and no way to tell them apart from here: the
//! client lost a response and retried, or somebody else has the credential.
//! RFC 6749 §4.1.2, RFC 6819 §5.2.2.3 and the OAuth 2.0 Security BCP §4.14 all
//! resolve that the same way — assume the leak and revoke everything issued
//! under the grant. That is what [`Issuer::revoke_family`] does: the grant, its
//! internal API key and every access and refresh token it ever minted, in one
//! batch. The legitimate client's next request fails and it re-runs its login;
//! the thief's does too, and it cannot.
//!
//! The device flow is the one exception, and it is in [`super::device`]: a
//! polling client re-sends the same device code by design.

use gproxy_store::{
    entity::oauth::{code, grant, token},
    operations::{
        CasOutcome,
        oauth::{ExchangeSource, IssuedToken, TokenExchange},
    },
};
use serde_json::json;

use super::{
    Issuer, IssuerError, authorize::verify_challenge, by_digest, by_grant, expires_at, join_scopes,
    mint_secret, stored_scopes,
};
use crate::{
    Result,
    audit::AuditEntry,
    auth::token_digest,
    dto::{DEVICE_GRANT_TYPE, RevokeRequest, TOKEN_TYPE, TokenRequest, TokenResponse},
    operations::random_id,
};
use gproxy_seaorm::BatchConnectionTrait;

impl<C: BatchConnectionTrait> Issuer<'_, C> {
    /// Redeem a code, a refresh token or a device code for a token pair.
    pub async fn token(&self, request: &TokenRequest) -> Result<TokenResponse> {
        self.token_at(request, crate::now_ms()).await
    }

    /// [`Issuer::token`] against a stated clock, so a test can sit on an
    /// expiry boundary.
    pub async fn token_at(&self, request: &TokenRequest, now_ms: i64) -> Result<TokenResponse> {
        match request.grant_type.trim() {
            "authorization_code" => self.exchange_code(request, now_ms).await,
            "refresh_token" => self.exchange_refresh(request, now_ms).await,
            DEVICE_GRANT_TYPE => self.exchange_device(request, now_ms).await,
            "" => Err(IssuerError::invalid_request("grant_type is required")),
            other => Err(IssuerError::unsupported_grant_type(format!(
                "`{other}` is not a supported grant type"
            ))),
        }
    }

    /// RFC 6749 §4.1.3.
    async fn exchange_code(&self, request: &TokenRequest, now_ms: i64) -> Result<TokenResponse> {
        let client = self.client(&request.client_id).await?;
        let presented = required(request.code.as_deref(), "code")?;
        let verifier = required(request.code_verifier.as_deref(), "code_verifier")?;
        // Validated against the registration rather than only against the code
        // row, which is also what keeps a device-issued code — whose recorded
        // redirect is a URN no registration can hold — out of this grant type.
        let redirect_uri = super::authorize::registered_redirect(
            &client,
            required(request.redirect_uri.as_deref(), "redirect_uri")?,
        )?;
        let Some(row) = self.code_row(presented).await? else {
            return Err(IssuerError::invalid_grant("the code is unknown"));
        };
        let grant = self.grant_of(&row.grant_id, &client.id).await?;
        // Replay first: a consumed code says the credential has been used
        // twice, whoever is holding it, and a wrong verifier on top of that is
        // not a reason to keep the family alive.
        if row.consumed_at_ms.is_some() {
            return self.replay(&grant, "oauth.token.code", now_ms).await;
        }
        verify_challenge(verifier, &row.code_challenge)?;
        let source = ExchangeSource::Code {
            id: row.id.clone(),
            hash: row.code_hash.clone(),
            redirect_uri,
            code_challenge: row.code_challenge.clone(),
        };
        match self.rotate(source, &grant, now_ms).await? {
            Some(response) => {
                self.record(
                    by_grant(
                        AuditEntry::new("oauth.token.code"),
                        &grant.user_id,
                        &grant.api_key_id,
                    )
                    .entity("oauth grant", &grant.id)
                    .detail(json!({ "clientId": grant.client_id })),
                )
                .await;
                Ok(response)
            }
            // The batch refused. Either somebody redeemed the same code in the
            // instant between the read above and this write — a replay by
            // another name — or the code expired, or the grant stopped being
            // live. Re-reading distinguishes them.
            None => {
                self.after_conflict(&grant, &row.id, true, "oauth.token.code", now_ms)
                    .await
            }
        }
    }

    /// RFC 6749 §6, with rotation.
    async fn exchange_refresh(&self, request: &TokenRequest, now_ms: i64) -> Result<TokenResponse> {
        let client = self.client(&request.client_id).await?;
        let presented = required(request.refresh_token.as_deref(), "refresh_token")?;
        let Some(row) = self.token_row(presented).await? else {
            return Err(IssuerError::invalid_grant("the refresh token is unknown"));
        };
        if row.kind != token::TokenKind::Refresh {
            // An access token presented as a refresh token is a client bug,
            // not a leak: it is not single-use and has not been replayed.
            return Err(IssuerError::invalid_grant(
                "that is an access token, not a refresh token",
            ));
        }
        let grant = self.grant_of(&row.grant_id, &client.id).await?;
        if row.consumed_at_ms.is_some() {
            return self.replay(&grant, "oauth.token.refresh", now_ms).await;
        }
        let source = ExchangeSource::Refresh {
            id: row.id.clone(),
            hash: row.token_hash.clone(),
        };
        match self.rotate(source, &grant, now_ms).await? {
            Some(response) => {
                self.record(
                    by_grant(
                        AuditEntry::new("oauth.token.refresh"),
                        &grant.user_id,
                        &grant.api_key_id,
                    )
                    .entity("oauth grant", &grant.id)
                    .detail(json!({
                        "clientId": grant.client_id,
                        "refreshCount": grant.refresh_count + 1,
                    })),
                )
                .await;
                Ok(response)
            }
            None => {
                self.after_conflict(&grant, &row.id, false, "oauth.token.refresh", now_ms)
                    .await
            }
        }
    }

    /// Mint a pair and consume `source` in one batch. `None` is
    /// [`CasOutcome::Conflict`]: the source was already spent, expired, or the
    /// grant is no longer live.
    pub(super) async fn rotate(
        &self,
        source: ExchangeSource,
        grant: &grant::Model,
        now_ms: i64,
    ) -> Result<Option<TokenResponse>> {
        let (access, access_digest) = mint_secret()?;
        let (refresh, refresh_digest) = mint_secret()?;
        let access_expires = expires_at(now_ms, self.config().oauth.access_ttl_secs);
        let refresh_expires = expires_at(now_ms, self.config().oauth.refresh_ttl_secs);
        // The store refuses a token that is born expired, and it is right to;
        // catching it here names the configuration rather than the statement.
        if access_expires <= now_ms || refresh_expires <= now_ms {
            return Err(IssuerError::server_error(
                "the configured OAuth token lifetime is zero",
            ));
        }
        let outcome = self
            .store()
            .oauth_grants()
            .exchange_tokens_many(vec![TokenExchange {
                source,
                grant_id: grant.id.clone(),
                client_id: grant.client_id.clone(),
                access: IssuedToken {
                    id: random_id()?,
                    hash: access_digest,
                    expires_at_ms: access_expires,
                },
                refresh: IssuedToken {
                    id: random_id()?,
                    hash: refresh_digest,
                    expires_at_ms: refresh_expires,
                },
                now_ms,
            }])
            .await?
            .into_iter()
            .next()
            .unwrap_or(CasOutcome::Conflict);
        if outcome != CasOutcome::Applied {
            return Ok(None);
        }
        Ok(Some(TokenResponse {
            access_token: access,
            token_type: TOKEN_TYPE.into(),
            expires_in: (access_expires - now_ms) / 1_000,
            refresh_token: refresh,
            scope: join_scopes(&stored_scopes(&grant.scopes)),
        }))
    }

    /// Work out what a refused batch meant, and answer accordingly.
    ///
    /// `code` selects which table to re-read. A row that is now consumed was
    /// redeemed by somebody between the read and the write, which is the same
    /// event as an outright replay and gets the same answer.
    async fn after_conflict(
        &self,
        grant: &grant::Model,
        id: &str,
        code: bool,
        action: &'static str,
        now_ms: i64,
    ) -> Result<TokenResponse> {
        let consumed = if code {
            self.store()
                .oauth_codes()
                .get_many(&[id.to_owned()])
                .await?
                .into_iter()
                .next()
                .flatten()
                .is_some_and(|row: code::Model| row.consumed_at_ms.is_some())
        } else {
            self.store()
                .oauth_tokens()
                .get_many(&[id.to_owned()])
                .await?
                .into_iter()
                .next()
                .flatten()
                .is_some_and(|row: token::Model| row.consumed_at_ms.is_some())
        };
        if consumed {
            return self.replay(grant, action, now_ms).await;
        }
        Err(IssuerError::invalid_grant(
            "the grant is expired, revoked or no longer eligible",
        ))
    }

    /// A one-shot credential was presented twice. Kill the family and refuse.
    async fn replay(
        &self,
        grant: &grant::Model,
        action: &'static str,
        now_ms: i64,
    ) -> Result<TokenResponse> {
        let revoked = self.revoke_family(&grant.id, now_ms).await?;
        let error = IssuerError::invalid_grant(
            "that credential has already been used; the authorization has been revoked",
        );
        self.record(
            by_grant(AuditEntry::new(action), &grant.user_id, &grant.api_key_id)
                .entity("oauth grant", &grant.id)
                .detail(json!({
                    "clientId": grant.client_id,
                    "replay": true,
                    "revoked": revoked,
                }))
                .failed(&error),
        )
        .await;
        Err(error)
    }

    /// Revoke a grant, its internal key and every token it issued. Idempotent:
    /// `false` means it was already revoked.
    pub(super) async fn revoke_family(&self, grant_id: &str, now_ms: i64) -> Result<bool> {
        Ok(self
            .store()
            .oauth_grants()
            .revoke_many(std::slice::from_ref(&grant_id.to_owned()), now_ms)
            .await?
            .first()
            .copied()
            .unwrap_or(false))
    }

    /// RFC 7009 §2.
    ///
    /// **Always succeeds**, including for a token that was never issued: §2.2
    /// is explicit that an invalid token is not an error, because the point of
    /// the endpoint is that the token no longer works afterwards and an
    /// unknown one already does not. Answering otherwise would turn the
    /// endpoint into an oracle for whether a guessed token exists.
    ///
    /// The boolean is for the audit trail, not for the client: a host answers
    /// 200 with an empty body either way.
    ///
    /// Revocation is **grant-wide**. RFC 7009 §2.1 leaves the scope of a
    /// revoked access token to the server; here a grant *is* the session, its
    /// tokens are rotations of one credential, and leaving the refresh token
    /// alive after "log this client out" would be a surprise in the dangerous
    /// direction.
    pub async fn revoke(&self, request: &RevokeRequest) -> Result<bool> {
        self.revoke_at(request, crate::now_ms()).await
    }

    /// [`Issuer::revoke`] against a stated clock.
    pub async fn revoke_at(&self, request: &RevokeRequest, now_ms: i64) -> Result<bool> {
        let presented = request.token.trim();
        if presented.is_empty() {
            return Err(IssuerError::invalid_request("token is required"));
        }
        let Some(row) = self.token_row(presented).await? else {
            return Ok(false);
        };
        let Some(grant) = self.grant_row(&row.grant_id).await? else {
            return Ok(false);
        };
        // A public client cannot prove who it is, so `client_id` can only ever
        // narrow a revocation, never authorize one. Presenting the token is
        // the authorization.
        if let Some(client_id) = request.client_id.as_deref().map(str::trim)
            && !client_id.is_empty()
            && client_id != grant.client_id
        {
            return Ok(false);
        }
        let revoked = self.revoke_family(&grant.id, now_ms).await?;
        self.record(
            by_grant(
                AuditEntry::new("oauth.revoke"),
                &grant.user_id,
                &grant.api_key_id,
            )
            .entity("oauth grant", &grant.id)
            .detail(json!({
                "clientId": grant.client_id,
                "kind": match row.kind {
                    token::TokenKind::Access => "access",
                    token::TokenKind::Refresh => "refresh",
                },
                "revoked": revoked,
            })),
        )
        .await;
        Ok(revoked)
    }

    /// The code row a presented plaintext hashes to.
    async fn code_row(&self, presented: &str) -> Result<Option<code::Model>> {
        by_digest(
            self.store().oauth_codes(),
            code::Column::CodeHash,
            &token_digest(presented),
        )
        .await
    }

    /// The token row a presented plaintext hashes to, of either kind:
    /// `token_hash` is unique across the table.
    async fn token_row(&self, presented: &str) -> Result<Option<token::Model>> {
        by_digest(
            self.store().oauth_tokens(),
            token::Column::TokenHash,
            &token_digest(presented),
        )
        .await
    }

    pub(super) async fn grant_row(&self, grant_id: &str) -> Result<Option<grant::Model>> {
        Ok(self
            .store()
            .oauth_grants()
            .get_many(&[grant_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten())
    }

    /// The grant behind a credential, refusing one that belongs to a different
    /// client.
    ///
    /// The same check is inside `exchange_tokens_many`'s statements, which are
    /// the authority; doing it here as well turns "the batch changed no rows"
    /// into a specific answer, and keeps a client from learning that somebody
    /// else's grant exists by watching which error it gets.
    pub(super) async fn grant_of(&self, grant_id: &str, client_id: &str) -> Result<grant::Model> {
        self.grant_row(grant_id)
            .await?
            .filter(|row| row.client_id == client_id)
            .ok_or_else(|| IssuerError::invalid_grant("the credential is unknown"))
    }
}

/// A parameter this grant type cannot do without.
pub(super) fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| IssuerError::invalid_request(format!("{name} is required")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_parameter_is_a_missing_one() {
        assert_eq!(required(Some(" abc "), "code").unwrap(), "abc");
        for absent in [None, Some(""), Some("   ")] {
            let error = required(absent, "code").unwrap_err();
            assert_eq!(error.code(), "invalid_request");
            assert!(error.to_string().contains("code is required"), "{error}");
        }
    }
}
