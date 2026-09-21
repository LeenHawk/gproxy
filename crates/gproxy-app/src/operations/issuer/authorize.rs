//! The authorization endpoint: validating a request before a consent screen
//! is drawn, and turning the person's decision into a code or an error
//! redirect.
//!
//! Two operations, and the split matters. [`Issuer::authorize_details`] runs
//! **every** check an approval would run and then returns what the screen
//! renders; [`Issuer::authorize`] runs them again and writes. A screen built
//! from `authorize_details` therefore never asks a user to approve something
//! that will fail afterwards, and re-checking at the write is not paranoia —
//! a client can be retired, or an allowlist edited, between the two requests,
//! and the second one is the one that creates a credential.

use gproxy_store::{
    entity::{
        identity::api_key,
        oauth::{client, code, grant},
    },
    operations::{CasOutcome, oauth::Authorization},
};
use sea_orm::Set;
use serde_json::{Value, json};

use super::{
    DEVICE_REDIRECT_URI, Issuer, IssuerError, by_caller, expires_at, mint_secret, parse_scopes,
    s256,
};
use crate::{
    Caller, Result,
    audit::AuditEntry,
    auth::{API_KEY_PREFIX, generate_api_key},
    dto::{
        AuthorizationDenied, AuthorizationIssued, AuthorizeDetails, AuthorizeOutcome,
        AuthorizeQuery, ConsentDecision,
    },
    operations::random_id,
};
use gproxy_seaorm::BatchConnectionTrait;

/// A validated authorization request: everything the write needs, already
/// checked and normalized.
pub(super) struct Validated {
    pub client: client::Model,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub state: Option<String>,
    pub code_challenge: String,
}

impl<C: BatchConnectionTrait> Issuer<'_, C> {
    /// Validate an authorization request and answer what a consent screen
    /// shows.
    ///
    /// Takes the [`Caller`] even though RFC 6749 §4.1.1 does not mention one,
    /// because the client allowlist is a property of the *user*: the same
    /// request is legitimate for one account and refused for another, and a
    /// screen that rendered before that was known would be offering a decision
    /// the instance will not honour.
    ///
    /// What is checked, and why each one is here rather than at the write:
    ///
    /// - `response_type` is `code`. There is no implicit flow to fall back to;
    /// - `code_challenge_method` is `S256` and the challenge is present.
    ///   `plain` is refused outright — every client here is public, so the
    ///   verifier is all that binds the code to the program that asked for it,
    ///   and a `plain` challenge is the verifier;
    /// - the redirect URI is **exactly** one of the registered strings;
    /// - the client exists, is enabled and is not retired;
    /// - the allowlist admits this user/client pair.
    pub async fn authorize_details(
        &self,
        caller: &Caller,
        query: &AuthorizeQuery,
    ) -> Result<AuthorizeDetails> {
        let validated = self.validate(caller, query).await?;
        Ok(AuthorizeDetails {
            client_id: validated.client.id,
            client_name: validated.client.name,
            scopes: validated.scopes,
            redirect_uri: validated.redirect_uri,
            state: validated.state,
            user_id: caller.user_id.clone(),
        })
    }

    /// Record the person's decision.
    ///
    /// [`ConsentDecision::Approve`] mints a 32-byte code and creates the
    /// grant's internal API key, the grant and the code as **one** batch
    /// (`issue_many`), which re-evaluates the client, the user and the
    /// allowlist inside the same statements — so a revocation racing the
    /// approval cannot land between the check and the insert.
    ///
    /// [`ConsentDecision::Deny`] writes nothing at all. It still validates the
    /// request first, because the refusal is delivered to the client's
    /// redirect URI and an unvalidated one would let anybody bounce an
    /// `error` — and the client's `state` — to a URL of their choosing.
    pub async fn authorize(
        &self,
        caller: &Caller,
        query: &AuthorizeQuery,
        decision: ConsentDecision,
    ) -> Result<AuthorizeOutcome> {
        self.authorize_at(caller, query, decision, crate::now_ms())
            .await
    }

    /// [`Issuer::authorize`] against a stated clock.
    pub async fn authorize_at(
        &self,
        caller: &Caller,
        query: &AuthorizeQuery,
        decision: ConsentDecision,
        now_ms: i64,
    ) -> Result<AuthorizeOutcome> {
        let validated = self.validate(caller, query).await?;
        if decision == ConsentDecision::Deny {
            self.record(
                by_caller(AuditEntry::new("oauth.authorize.deny"), caller)
                    .entity("oauth client", &validated.client.id)
                    .detail(json!({ "scopes": validated.scopes })),
            )
            .await;
            return Ok(AuthorizeOutcome::Denied(AuthorizationDenied {
                redirect_uri: validated.redirect_uri,
                state: validated.state,
                error: "access_denied".into(),
                error_description: "the account holder refused the authorization".into(),
            }));
        }
        let issued = self.issue(caller, &validated, None, now_ms).await?;
        Ok(AuthorizeOutcome::Issued(AuthorizationIssued {
            code: issued,
            redirect_uri: validated.redirect_uri,
            state: validated.state,
        }))
    }

    /// Everything both operations check, in one place.
    pub(super) async fn validate(
        &self,
        caller: &Caller,
        query: &AuthorizeQuery,
    ) -> Result<Validated> {
        if query.response_type.trim() != "code" {
            return Err(IssuerError::unsupported_response_type(
                "response_type must be `code`",
            ));
        }
        let challenge = query.code_challenge.trim();
        // S256 only, as `oauth_codes` documents. `plain` puts the verifier in
        // the authorization request, where every browser, proxy and log along
        // the redirect can read it, which defeats the point of PKCE.
        //
        // The *spelling* is read leniently — trimmed and case-insensitive —
        // because a form value often arrives padded and some clients
        // lowercase it. That costs nothing: there is one transformation, and
        // recognising its name in another case does not select a weaker one.
        if !query
            .code_challenge_method
            .trim()
            .eq_ignore_ascii_case("S256")
        {
            return Err(IssuerError::invalid_request(
                "code_challenge_method must be `S256`",
            ));
        }
        if challenge.is_empty() {
            return Err(IssuerError::invalid_request("code_challenge is required"));
        }
        let client = self.client(&query.client_id).await?;
        let redirect_uri = registered_redirect(&client, &query.redirect_uri)?;
        self.allow_client(&caller.user_id, &client.id).await?;
        Ok(Validated {
            client,
            redirect_uri,
            scopes: parse_scopes(&query.scope)?,
            state: query
                .state
                .as_deref()
                .map(str::to_owned)
                .filter(|state| !state.is_empty()),
            code_challenge: challenge.to_owned(),
        })
    }

    /// Create the internal key, the grant and the code as one batch, and
    /// answer the code's plaintext.
    ///
    /// `device` names a pending device authorization to reserve and approve in
    /// the same transaction; None is the browser flow.
    ///
    /// The internal key is bound to **the caller's own binding**: their
    /// organization, team and subscription, whatever they are. In practice the
    /// consent screen is a portal session, which carries none of those — a
    /// session acts as the person, not as a key — so the usual outcome is an
    /// unbound key that pays from the user's own chain. When an API-key caller
    /// drives the approval instead, the grant inherits that key's scope,
    /// because a program authorized inside an organization must not escape it
    /// by going through OAuth. The key's plaintext is generated and dropped on
    /// the floor: nothing can ever present it, since authentication refuses a
    /// `kind = OAuth` key offered as a bearer key, and the row needs only a
    /// unique digest.
    pub(super) async fn issue(
        &self,
        caller: &Caller,
        validated: &Validated,
        device: Option<gproxy_store::operations::oauth::DeviceApproval>,
        now_ms: i64,
    ) -> Result<String> {
        let (code, code_digest) = mint_secret()?;
        let key_id = random_id()?;
        let grant_id = random_id()?;
        let (_plaintext, prefix, key_hash) = generate_api_key(API_KEY_PREFIX)?;
        let scopes: Value = json!(validated.scopes);
        let authorization = Authorization {
            api_key: api_key::ActiveModel {
                id: Set(key_id.clone()),
                user_id: Set(caller.user_id.clone()),
                subscription_id: Set(caller.subscription_id.clone()),
                organization_id: Set(caller.organization_id.clone()),
                team_id: Set(caller.team_id.clone()),
                name: Set(format!("oauth: {}", validated.client.name)),
                kind: Set(api_key::ApiKeyKind::OAuth),
                key_hash: Set(key_hash),
                prefix: Set(prefix),
                secret: Set(None),
                expires_at_ms: Set(None),
                enabled: Set(true),
            },
            grant: grant::ActiveModel {
                id: Set(grant_id.clone()),
                user_id: Set(caller.user_id.clone()),
                api_key_id: Set(key_id.clone()),
                client_id: Set(validated.client.id.clone()),
                scopes: Set(scopes.clone()),
                subject: Set(caller.user_id.clone()),
                account_id: Set(caller.organization_id.clone()),
                created_at_ms: Set(now_ms),
                revoked_at_ms: Set(None),
                logged_in_at_ms: Set(None),
                last_refreshed_at_ms: Set(None),
                refresh_count: Set(0),
                refresh_expires_at_ms: Set(None),
            },
            code: code::ActiveModel {
                id: Set(random_id()?),
                code_hash: Set(code_digest),
                grant_id: Set(grant_id.clone()),
                redirect_uri: Set(validated.redirect_uri.clone()),
                code_challenge: Set(validated.code_challenge.clone()),
                created_at_ms: Set(now_ms),
                expires_at_ms: Set(expires_at(now_ms, self.config().oauth.code_ttl_secs)),
                consumed_at_ms: Set(None),
                consumed_by: Set(None),
            },
            device,
            now_ms,
        };
        let outcome = self
            .store()
            .oauth_grants()
            .issue_many(vec![authorization])
            .await?
            .into_iter()
            .next()
            .unwrap_or(CasOutcome::Conflict);
        if outcome != CasOutcome::Applied {
            // The statements re-checked the client, the user, the allowlist,
            // the subscription and — for a device flow — that the pending
            // authorization was still pending. One of them changed since
            // `validate` ran, which is exactly the race this batch exists to
            // lose safely.
            let error =
                IssuerError::access_denied("the authorization is no longer permitted; start again");
            self.record(
                by_caller(AuditEntry::new("oauth.authorize.approve"), caller)
                    .entity("oauth client", &validated.client.id)
                    .failed(&error),
            )
            .await;
            return Err(error);
        }
        self.record(
            by_caller(AuditEntry::new("oauth.authorize.approve"), caller)
                .entity("oauth grant", &grant_id)
                .detail(json!({
                    "clientId": validated.client.id,
                    "scopes": validated.scopes,
                    "apiKeyId": key_id,
                    "device": authorization_kind(validated),
                })),
        )
        .await;
        Ok(code)
    }
}

/// Whether the code was minted for a redirect or for a device.
fn authorization_kind(validated: &Validated) -> &'static str {
    if validated.redirect_uri == DEVICE_REDIRECT_URI {
        "device"
    } else {
        "browser"
    }
}

/// The registered redirect URI equal to `presented`, or `invalid_request`.
///
/// **Exact string equality, and nothing else.** Not a prefix, not a
/// same-origin test, not a normalization pass. Every looser rule has the same
/// failure: a client registered for `https://app.example/cb` would also accept
/// `https://app.example/cb.attacker.example`, `https://app.example/cb/../../x`
/// or `https://app.example/cb?next=//evil`, and each of those is a URL an
/// attacker controls that receives the authorization code. RFC 6749 §3.1.2.3
/// asks for exactly this, and the registry refuses to store a `*` so that a
/// registration cannot ask for anything else.
///
/// Returning the *registered* string rather than the presented one means the
/// row and the redirect carry the value the operator wrote, byte for byte,
/// even if the two differed in some way the comparison would have to allow.
pub(super) fn registered_redirect(client: &client::Model, presented: &str) -> Result<String> {
    let presented = presented.trim();
    if presented.is_empty() {
        return Err(IssuerError::invalid_request("redirect_uri is required"));
    }
    client
        .redirect_uris
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find(|registered| *registered == presented)
        .map(str::to_owned)
        .ok_or_else(|| {
            IssuerError::invalid_request(format!(
                "`{presented}` is not a registered redirect URI for `{}`",
                client.id
            ))
        })
}

/// The S256 challenge of `verifier`, for comparison against a stored one.
///
/// RFC 7636 §4.1 requires 43–128 characters from the unreserved set; a shorter
/// verifier is refused rather than hashed, because a client that sends eight
/// characters has not implemented PKCE, it has implemented a password.
pub(super) fn verify_challenge(verifier: &str, challenge: &str) -> Result<()> {
    if !(43..=128).contains(&verifier.len())
        || !verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
    {
        return Err(IssuerError::invalid_grant(
            "code_verifier must be 43 to 128 unreserved characters",
        ));
    }
    if s256(verifier) != challenge {
        return Err(IssuerError::invalid_grant(
            "code_verifier does not match the code_challenge",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_row(uris: Value) -> client::Model {
        client::Model {
            id: "cli".into(),
            name: "CLI".into(),
            redirect_uris: uris,
            enabled: true,
            deleted_at_ms: None,
        }
    }

    #[test]
    fn a_redirect_uri_matches_only_itself() {
        let client = client_row(json!([
            "http://127.0.0.1:1455/callback",
            "https://app.example/cb"
        ]));
        assert_eq!(
            registered_redirect(&client, "http://127.0.0.1:1455/callback").unwrap(),
            "http://127.0.0.1:1455/callback"
        );
        // Every one of these is a prefix, a suffix or a normalization away.
        for near in [
            "http://127.0.0.1:1455/callback/",
            "http://127.0.0.1:1455/callback?x=1",
            "http://127.0.0.1:1455/callback.attacker.example",
            "http://127.0.0.1:1455/callback/../../elsewhere",
            "http://127.0.0.1:1456/callback",
            "https://127.0.0.1:1455/callback",
            "HTTP://127.0.0.1:1455/callback",
            "https://app.example/cb#frag",
        ] {
            let error = registered_redirect(&client, near).unwrap_err();
            assert_eq!(error.code(), "invalid_request", "{near} was accepted");
        }
    }

    #[test]
    fn a_client_with_no_registered_redirect_accepts_none() {
        let error =
            registered_redirect(&client_row(json!([])), "https://app.example/cb").unwrap_err();
        assert_eq!(error.status_code(), 400);
        let error =
            registered_redirect(&client_row(json!(null)), "https://app.example/cb").unwrap_err();
        assert_eq!(error.status_code(), 400);
        // And an absent one is named as missing rather than as unregistered.
        let error = registered_redirect(&client_row(json!(["x://y"])), "  ").unwrap_err();
        assert!(error.to_string().contains("required"), "{error}");
    }

    #[test]
    fn a_verifier_must_be_the_rfcs_shape_before_it_is_hashed() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(verify_challenge(verifier, challenge).is_ok());
        assert!(verify_challenge(verifier, "something-else").is_err());
        for bad in ["short", &"a".repeat(129), &"+".repeat(43)] {
            let error = verify_challenge(bad, challenge).unwrap_err();
            assert_eq!(error.code(), "invalid_grant");
        }
    }
}
