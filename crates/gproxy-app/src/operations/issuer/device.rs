//! RFC 8628, the device authorization grant: for a program on a machine with
//! no browser, or no keyboard worth typing a URL on.
//!
//! The device asks for a pair of codes, shows the person the short one, and
//! polls. The person opens the portal on whatever device they do have, types
//! the short code and approves. The poll then returns tokens.
//!
//! # How this reuses the code flow rather than duplicating it
//!
//! An approval mints an ordinary authorization code and the store's
//! `issue_many` reserves and approves the pending device row in the *same*
//! batch. So a device authorization is a normal grant with an unusual way of
//! getting consent, and every rule about grants applies to it unchanged.
//!
//! Two columns make that work without inventing a second shape:
//!
//! - the code's `redirect_uri` is [`DEVICE_REDIRECT_URI`], a URN that no
//!   registration can hold, so a device-issued code can never be redeemed
//!   through `authorization_code`;
//! - the code's `code_challenge` is the base64url of `device_code_hash`, which
//!   *is* the S256 challenge of the device code, since both are the base64url
//!   of the same SHA-256. The device code is therefore the PKCE verifier, for
//!   free and with no extra column: only the device that was issued it can
//!   produce a value that hashes to the stored challenge.
//!
//! # Replay is not a revocation here
//!
//! A polling client re-sends the same device code by design, and a response
//! that was lost in transit is indistinguishable from one that arrived. So a
//! consumed device authorization answers `invalid_grant` and stops there — it
//! does **not** revoke the grant the way a replayed refresh token does. The
//! device code is spent either way, so nothing is gained by taking the tokens
//! with it, and a client that lost one response would otherwise be locked out
//! of an account it had just legitimately connected.
//!
//! # `slow_down` is never emitted
//!
//! RFC 8628 §3.5 allows it for a client polling faster than `interval`, but
//! answering it means keeping a per-device timestamp of the last poll — a
//! write on every poll of every device, for a code that only ever tells a
//! well-behaved client something it already knows. Rate limiting the endpoint
//! is the host's job and it is the same defence.

use base64::Engine;
use gproxy_store::{
    entity::oauth::{code, device},
    operations::{
        CasOutcome,
        oauth::{DeviceApproval, ExchangeSource},
    },
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;

use super::{
    DEVICE_POLL_INTERVAL_SECS, DEVICE_REDIRECT_URI, DEVICE_VERIFICATION_PATH, Issuer, IssuerError,
    IssuerOrigin, authorize::Validated, by_caller, by_digest, by_grant, expires_at, mint_secret,
    parse_scopes, s256, stored_scopes,
};
use crate::{
    Caller, Result,
    audit::AuditEntry,
    auth::{random_bytes, token_digest},
    dto::{
        ConsentDecision, DeviceCodeRequest, DeviceCodeResponse, DeviceDecided, DeviceDetails,
        TokenRequest, TokenResponse,
    },
    operations::random_id,
};
use gproxy_seaorm::BatchConnectionTrait;

/// The alphabet of a user code: uppercase letters and digits with `I`, `O`,
/// `0` and `1` removed, because a person is reading this off one screen and
/// typing it into another. Exactly 32 symbols, so a uniformly random byte maps
/// onto it without modulo bias.
const USER_CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Eight symbols: 32^8 ≈ 1.1 × 10^12, against a code that lives for fifteen
/// minutes and is typed by hand.
const USER_CODE_LENGTH: usize = 8;

impl<C: BatchConnectionTrait> Issuer<'_, C> {
    /// RFC 8628 §3.1: start a device authorization.
    pub async fn device_code(
        &self,
        request: &DeviceCodeRequest,
        origin: &IssuerOrigin,
    ) -> Result<DeviceCodeResponse> {
        self.device_code_at(request, origin, crate::now_ms()).await
    }

    /// [`Issuer::device_code`] against a stated clock.
    ///
    /// The verification URI hangs off the instance **origin**, not off the
    /// issuer mount: the console is one application however many mounts the
    /// data plane answers at, and sending a person to
    /// `https://host/openai/v1/console/device` would be sending them nowhere.
    pub async fn device_code_at(
        &self,
        request: &DeviceCodeRequest,
        origin: &IssuerOrigin,
        now_ms: i64,
    ) -> Result<DeviceCodeResponse> {
        let client = self.client(&request.client_id).await?;
        let scopes = parse_scopes(&request.scope)?;
        let (device_code, device_digest) = mint_secret()?;
        let user_code = mint_user_code()?;
        let expires_at_ms = expires_at(now_ms, self.config().oauth.device_ttl_secs);
        self.store()
            .oauth_devices()
            .create_many(vec![device::ActiveModel {
                id: Set(random_id()?),
                device_code_hash: Set(device_digest),
                user_code: Set(user_code.clone()),
                client_id: Set(client.id.clone()),
                scopes: Set(json!(scopes)),
                grant_id: Set(None),
                created_at_ms: Set(now_ms),
                expires_at_ms: Set(expires_at_ms),
                approved_at_ms: Set(None),
                denied_at_ms: Set(None),
                consumed_at_ms: Set(None),
                approval_receipt: Set(None),
                authorization_payload: Set(None),
            }])
            .await?;
        let grouped = group_user_code(&user_code);
        self.record(
            AuditEntry::new("oauth.device.start")
                .entity("oauth client", &client.id)
                .detail(json!({ "userCode": grouped, "scopes": scopes })),
        )
        .await;
        Ok(DeviceCodeResponse {
            device_code,
            // The grouped spelling is what the person reads; lookups normalize,
            // so it does not matter which one they type back.
            user_code: grouped.clone(),
            verification_uri: origin.instance(DEVICE_VERIFICATION_PATH),
            // Every character of a grouped code is unreserved, so it needs no
            // percent-encoding to sit in a query string.
            verification_uri_complete: format!(
                "{}?user_code={grouped}",
                origin.instance(DEVICE_VERIFICATION_PATH)
            ),
            expires_in: (expires_at_ms - now_ms) / 1_000,
            interval: DEVICE_POLL_INTERVAL_SECS,
        })
    }

    /// What the approval page renders for a code a person typed in.
    pub async fn device_details(&self, user_code: &str) -> Result<DeviceDetails> {
        self.device_details_at(user_code, crate::now_ms()).await
    }

    /// [`Issuer::device_details`] against a stated clock.
    pub async fn device_details_at(&self, user_code: &str, now_ms: i64) -> Result<DeviceDetails> {
        let row = self.pending_device(user_code, now_ms).await?;
        let client = self.client(&row.client_id).await?;
        Ok(DeviceDetails {
            client_id: client.id,
            client_name: client.name,
            user_code: row.user_code,
            scopes: stored_scopes(&row.scopes),
            expires_at_ms: row.expires_at_ms,
        })
    }

    /// Record the person's decision on a pending device authorization.
    ///
    /// An approval goes through the same `issue_many` batch the browser flow
    /// uses, which reserves the device row with a one-shot receipt before it
    /// writes anything — so two people approving the same code at once produce
    /// one grant, not two.
    pub async fn device_decision(
        &self,
        caller: &Caller,
        user_code: &str,
        decision: ConsentDecision,
    ) -> Result<DeviceDecided> {
        self.device_decision_at(caller, user_code, decision, crate::now_ms())
            .await
    }

    /// [`Issuer::device_decision`] against a stated clock.
    pub async fn device_decision_at(
        &self,
        caller: &Caller,
        user_code: &str,
        decision: ConsentDecision,
        now_ms: i64,
    ) -> Result<DeviceDecided> {
        let row = self.pending_device(user_code, now_ms).await?;
        let client = self.client(&row.client_id).await?;
        self.allow_client(&caller.user_id, &client.id).await?;
        if decision == ConsentDecision::Deny {
            let denied = self
                .store()
                .oauth_devices()
                .deny_pending_many(std::slice::from_ref(&row.id), now_ms)
                .await?
                .into_iter()
                .next()
                .unwrap_or(CasOutcome::Conflict);
            if denied != CasOutcome::Applied {
                return Err(IssuerError::invalid_grant(
                    "that device authorization has already been decided",
                ));
            }
            self.record(
                by_caller(AuditEntry::new("oauth.device.deny"), caller)
                    .entity("oauth client", &client.id)
                    .detail(json!({ "userCode": row.user_code })),
            )
            .await;
            return Ok(DeviceDecided {
                user_code: row.user_code,
                approved: false,
            });
        }
        let validated = Validated {
            client,
            redirect_uri: DEVICE_REDIRECT_URI.to_owned(),
            scopes: stored_scopes(&row.scopes),
            state: None,
            // The S256 challenge of the device code, without ever holding the
            // device code: both are the base64url of the same SHA-256.
            code_challenge: base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(&row.device_code_hash),
        };
        // The code's plaintext is deliberately dropped. Nobody is waiting for
        // it: the polling device proves itself with the device code, and the
        // exchange reads the code row's own columns.
        let _code = self
            .issue(
                caller,
                &validated,
                Some(DeviceApproval {
                    id: row.id.clone(),
                    authorization_payload: None,
                }),
                now_ms,
            )
            .await?;
        self.record(
            by_caller(AuditEntry::new("oauth.device.approve"), caller)
                .entity("oauth client", &validated.client.id)
                .detail(json!({ "userCode": row.user_code })),
        )
        .await;
        Ok(DeviceDecided {
            user_code: row.user_code,
            approved: true,
        })
    }

    /// RFC 8628 has no cancellation, but a device that is shutting down should
    /// be able to retire the code it printed rather than leave it usable until
    /// it expires. Presenting the device code is the authorization, so this
    /// needs no caller.
    ///
    /// Like [`Issuer::revoke`], it never fails on an unknown code: the outcome
    /// a caller wants is "this code no longer works", and an unknown one
    /// already does not.
    pub async fn device_cancel(&self, client_id: &str, device_code: &str) -> Result<DeviceDecided> {
        self.device_cancel_at(client_id, device_code, crate::now_ms())
            .await
    }

    /// [`Issuer::device_cancel`] against a stated clock.
    pub async fn device_cancel_at(
        &self,
        client_id: &str,
        device_code: &str,
        now_ms: i64,
    ) -> Result<DeviceDecided> {
        let client = self.client(client_id).await?;
        let row = self.device_row(device_code).await?;
        let Some(row) = row.filter(|row| row.client_id == client.id) else {
            return Ok(DeviceDecided {
                user_code: String::new(),
                approved: false,
            });
        };
        self.store()
            .oauth_devices()
            .deny_pending_many(std::slice::from_ref(&row.id), now_ms)
            .await?;
        self.record(
            AuditEntry::new("oauth.device.cancel")
                .entity("oauth client", &client.id)
                .detail(json!({ "userCode": row.user_code })),
        )
        .await;
        Ok(DeviceDecided {
            user_code: row.user_code,
            approved: false,
        })
    }

    /// RFC 8628 §3.4: one poll.
    pub(super) async fn exchange_device(
        &self,
        request: &TokenRequest,
        now_ms: i64,
    ) -> Result<TokenResponse> {
        let client = self.client(&request.client_id).await?;
        let presented = super::token::required(request.device_code.as_deref(), "device_code")?;
        let row = self
            .device_row(presented)
            .await?
            .filter(|row| row.client_id == client.id)
            .ok_or_else(|| IssuerError::invalid_grant("the device code is unknown"))?;
        // The order is the RFC's §3.5 list, and it matters: an expired code
        // that was also denied should say expired, because that is what the
        // device can act on.
        if row.expires_at_ms <= now_ms {
            return Err(IssuerError::expired_token(
                "the device code has expired; start again",
            ));
        }
        if row.denied_at_ms.is_some() {
            return Err(IssuerError::access_denied(
                "the account holder refused the authorization",
            ));
        }
        if row.consumed_at_ms.is_some() {
            // Not a family revocation: see the module note.
            return Err(IssuerError::invalid_grant(
                "the device code has already been redeemed",
            ));
        }
        let Some(grant_id) = row
            .grant_id
            .clone()
            .filter(|_| row.approved_at_ms.is_some())
        else {
            return Err(IssuerError::authorization_pending(
                "the account holder has not decided yet",
            ));
        };
        let grant = self.grant_of(&grant_id, &client.id).await?;
        let Some(authorization) = self.unspent_code(&grant_id).await? else {
            return Err(IssuerError::invalid_grant(
                "the device code has already been redeemed",
            ));
        };
        // The device code *is* the PKCE verifier. Redundant with the digest
        // lookup above and kept anyway: it is the check that would still hold
        // if a future path reached this code row by another route.
        if s256(presented) != authorization.code_challenge {
            return Err(IssuerError::invalid_grant(
                "the device code does not match its authorization",
            ));
        }
        let source = ExchangeSource::Code {
            id: authorization.id,
            hash: authorization.code_hash,
            redirect_uri: authorization.redirect_uri,
            code_challenge: authorization.code_challenge,
        };
        let Some(response) = self.rotate(source, &grant, now_ms).await? else {
            return Err(IssuerError::invalid_grant(
                "the device authorization is no longer eligible",
            ));
        };
        // After the tokens, and outside their batch. A failure here leaves the
        // device row unmarked, which costs nothing: the code it points at is
        // already consumed, so the next poll finds no unspent code and answers
        // `invalid_grant` just the same.
        self.consume_device(&row.id, now_ms).await?;
        self.record(
            by_grant(
                AuditEntry::new("oauth.token.device"),
                &grant.user_id,
                &grant.api_key_id,
            )
            .entity("oauth grant", &grant.id)
            .detail(json!({ "clientId": grant.client_id, "userCode": row.user_code })),
        )
        .await;
        Ok(response)
    }

    /// A device authorization that is still waiting for a decision.
    async fn pending_device(&self, user_code: &str, now_ms: i64) -> Result<device::Model> {
        let normalized = normalize_user_code(user_code);
        if normalized.len() != USER_CODE_LENGTH {
            return Err(IssuerError::invalid_grant("that code does not exist"));
        }
        let row = self
            .store()
            .oauth_devices()
            .query(device::Entity::find().filter(device::Column::UserCode.eq(normalized)))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| IssuerError::invalid_grant("that code does not exist"))?;
        if row.expires_at_ms <= now_ms {
            return Err(IssuerError::expired_token("that code has expired"));
        }
        if row.approved_at_ms.is_some()
            || row.denied_at_ms.is_some()
            || row.consumed_at_ms.is_some()
        {
            return Err(IssuerError::invalid_grant(
                "that code has already been decided",
            ));
        }
        Ok(row)
    }

    async fn device_row(&self, device_code: &str) -> Result<Option<device::Model>> {
        by_digest(
            self.store().oauth_devices(),
            device::Column::DeviceCodeHash,
            &token_digest(device_code),
        )
        .await
    }

    /// The authorization code a device approval minted, if it has not been
    /// redeemed. A device grant has exactly one.
    async fn unspent_code(&self, grant_id: &str) -> Result<Option<code::Model>> {
        Ok(self
            .store()
            .oauth_codes()
            .query(
                code::Entity::find()
                    .filter(code::Column::GrantId.eq(grant_id))
                    .filter(code::Column::ConsumedAtMs.is_null()),
            )
            .await?
            .into_iter()
            .next())
    }

    async fn consume_device(&self, id: &str, now_ms: i64) -> Result<()> {
        self.store()
            .oauth_devices()
            .update_where_many(vec![
                device::Entity::update_many()
                    .col_expr(
                        device::Column::ConsumedAtMs,
                        sea_orm::sea_query::Expr::val(now_ms),
                    )
                    .filter(device::Column::Id.eq(id))
                    .filter(device::Column::ConsumedAtMs.is_null()),
            ])
            .await?;
        Ok(())
    }
}

/// Eight random symbols, stored without separators.
fn mint_user_code() -> Result<String> {
    let bytes = random_bytes::<USER_CODE_LENGTH>()?;
    Ok(bytes
        .iter()
        .map(|byte| USER_CODE_ALPHABET[usize::from(*byte) % USER_CODE_ALPHABET.len()] as char)
        .collect())
}

/// The stored code as a person reads it: `ABCD-EFGH`.
fn group_user_code(code: &str) -> String {
    let (head, tail) = code.split_at(code.len() / 2);
    format!("{head}-{tail}")
}

/// A typed code as it is stored: uppercase, and everything that is not in the
/// alphabet dropped.
///
/// That covers the separator, spaces a person added, and a trailing newline
/// from a paste. It cannot repair a genuine misreading, which is why the
/// alphabet has no `I`, `O`, `0` or `1` to misread in the first place.
fn normalize_user_code(input: &str) -> String {
    input
        .chars()
        .map(|ch| ch.to_ascii_uppercase())
        .filter(|ch| USER_CODE_ALPHABET.contains(&(*ch as u8)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_user_code_avoids_every_symbol_a_person_could_confuse() {
        for forbidden in ['I', 'O', '0', '1'] {
            assert!(
                !USER_CODE_ALPHABET.contains(&(forbidden as u8)),
                "{forbidden} is in the alphabet"
            );
        }
        // 32 symbols means a uniformly random byte maps on without bias.
        assert_eq!(USER_CODE_ALPHABET.len(), 32);
        assert_eq!(256 % USER_CODE_ALPHABET.len(), 0);
    }

    #[test]
    fn a_minted_code_is_eight_symbols_of_the_alphabet() {
        let code = mint_user_code().unwrap();
        assert_eq!(code.len(), USER_CODE_LENGTH);
        assert!(
            code.bytes().all(|b| USER_CODE_ALPHABET.contains(&b)),
            "{code}"
        );
        assert_eq!(group_user_code(&code).len(), USER_CODE_LENGTH + 1);
    }

    #[test]
    fn every_spelling_of_one_code_normalizes_to_the_stored_form() {
        for spelling in [
            "ABCD-EFGH",
            "abcd-efgh",
            "ABCDEFGH",
            "  ABCD EFGH\n",
            "abcd—efgh",
        ] {
            assert_eq!(normalize_user_code(spelling), "ABCDEFGH", "{spelling}");
        }
    }

    #[test]
    fn normalization_drops_rather_than_guesses() {
        // `0`, `1`, `I` and `O` are not in the alphabet, so a code containing
        // one cannot be repaired into a different, valid code.
        assert_eq!(normalize_user_code("A0B1CIDO"), "ABCD");
        assert!(normalize_user_code("").is_empty());
    }

    #[test]
    fn a_device_codes_challenge_is_its_own_s256() {
        // The approval writes base64url(device_code_hash); the poll computes
        // S256(device_code). They have to be the same string.
        let (device_code, digest) = mint_secret().unwrap();
        let stored = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest);
        assert_eq!(stored, s256(&device_code));
    }
}
