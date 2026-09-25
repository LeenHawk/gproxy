//! GProxy as an **OAuth authorization server for downstream clients**.
//!
//! This is the direction that is easy to get backwards, so the store says it
//! verbatim and so does this module: these endpoints issue *gproxy's own*
//! credentials to a program running on the user's machine — an editor
//! extension, a coding CLI, a script. They are not how gproxy logs in to an
//! upstream; that is `gproxy-sdk`'s `login()`, which speaks somebody else's
//! OAuth as a client. Nothing in this module ever talks to a provider.
//!
//! ```text
//!   Claude Code ──authorize/token──▶ gproxy  (this module: the issuer)
//!                                      │
//!                                      └──login()──▶ OpenAI   (the sdk: the client)
//! ```
//!
//! # What a grant is
//!
//! One authorization binds a **user** and an **internal API key** of
//! `kind = OAuth`, created in the same transaction. Everything downstream then
//! works on keys, not on a second parallel identity: the budget chain, the
//! credential-visibility boundary and the permission subject all come from
//! that key row exactly as they would for a key the user minted by hand. The
//! key is never presentable — [`Authenticator`](crate::Authenticator) refuses a
//! `kind = OAuth` key offered as a bearer key — so the only way to use it is
//! to present an access token, which resolves the whole chain in one statement
//! (`resolve_access_many`).
//!
//! **What an OAuth caller may then do is not decided here.** The operation
//! baseline — list models, count tokens, generate, stream, compact, and
//! nothing else unless the client is named in
//! [`AppConfig.oauth.cli_client_ids`](crate::config::OAuthIssuerConfig) — lives
//! in [`admission::permission`](crate::admission::permission), because it is a
//! property of every request the token makes and not of the moment it was
//! minted. Widening it must not require re-issuing tokens, and narrowing it
//! must take effect on the tokens that already exist.
//!
//! # The rules this issuer does not bend
//!
//! **PKCE S256, always.** `plain` is refused and so is an absent challenge.
//! Every client here is public, with no secret to prove with, so the verifier
//! is the only thing standing between a leaked code and a token.
//!
//! **Redirect URIs match exactly.** Not a prefix, not a wildcard, not
//! "same origin". Prefix matching is the classic open redirect: a client
//! registered for `https://app.example/cb` would also accept
//! `https://app.example/cb.attacker.example` or
//! `https://app.example/cb/../../elsewhere`, and every one of those receives
//! the authorization code. The registry refuses to store a `*` for the same
//! reason ([`OAuthClients::redirect_uris`](crate::operations::OAuthClients)).
//!
//! **Codes, refresh tokens and device codes are single-use.** The consumption
//! and the replacement are one atomic batch in `exchange_tokens_many`, so two
//! concurrent redemptions cannot both succeed; the loser sees `CasOutcome::
//! Conflict`.
//!
//! **A replayed one-shot credential kills the whole family.** If a refresh
//! token that has already been consumed is presented again, either the client
//! has it twice or somebody else has it too, and there is no way to tell which
//! from here. RFC 6819 §5.2.2.3 and the OAuth 2.0 Security BCP both answer the
//! same way: revoke the entire grant. [`Issuer::token`] does exactly that — the
//! grant, its internal key and every issued token die together — and the same
//! holds for a replayed authorization code (RFC 6749 §4.1.2). The device flow
//! is deliberately the exception: a polling client legitimately re-sends the
//! same device code, so a consumed device authorization is `invalid_grant` and
//! nothing more.
//!
//! **Tokens exist in exactly one response.** 32 bytes of entropy, base64url
//! without padding; the row keeps the SHA-256 and nothing else. A lost token
//! is replaced, never recovered, and no operation on this type returns one it
//! did not just mint.
//!
//! # What this module is not
//!
//! It is not HTTP. There is no routing, no form parsing, no status code and no
//! `Location` header here — a host maps its transport onto these operations,
//! and the two hosts must produce the same answers from the same types. What
//! *is* here is [`error_body`], so both of them render a failure identically.
//!
//! # Revisions
//!
//! None of this bumps `settings.config_revision`, which is the same decision
//! [`Sessions`](crate::operations::Sessions) and [`crate::audit`] made. A grant
//! is resolved by a database read on every request, so a peer's stale
//! [`AppData`] cannot admit a revoked token or refuse a live one. Bumping the
//! revision per exchange would instead make **every refresh invalidate every
//! peer's snapshot**, which on a fleet with short-lived access tokens is a
//! reload storm in exchange for nothing. The one OAuth operation that *is*
//! configuration — retiring a client, which changes the allowlist `AppData`
//! holds — lives in [`OAuthClients::retire`](crate::operations::OAuthClients)
//! and commits a revision there.

mod authorize;
mod device;
mod error;
mod token;

pub use error::{IssuerError, IssuerErrorCode, error_body};

use base64::Engine;
use gproxy_sdk::Gproxy;
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Store, entity::oauth::client};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    AppConfig, AppData, AppError, Caller, Result,
    audit::{Audit, AuditEntry},
    auth::{random_bytes, token_digest},
    dto::{AuthorizationServerMetadata, DEVICE_GRANT_TYPE},
};

/// The page a person opens to approve a device authorization, relative to the
/// **instance root** rather than to the issuer mount: the portal is one
/// application however many mounts the data plane answers at.
const DEVICE_VERIFICATION_PATH: &str = "/portal/device";

/// RFC 8628 §3.2's `interval`. Five seconds is the RFC's own default and what
/// every client assumes when the field is absent.
const DEVICE_POLL_INTERVAL_SECS: i64 = 5;

/// The `redirect_uri` recorded on a code that a device approval minted.
///
/// A device authorization has no redirect at all — that is the point of the
/// flow — but the code row's column is not nullable and the store matches on
/// it. This sentinel is safe because it **cannot collide with a registered
/// redirect URI**: registration requires an absolute URI containing `://`
/// ([`OAuthClients`](crate::operations::OAuthClients)), and this is a URN. So
/// a device-issued code can never be redeemed through the
/// `authorization_code` grant, which checks the presented redirect against the
/// registration.
const DEVICE_REDIRECT_URI: &str = DEVICE_GRANT_TYPE;

/// The OAuth issuer over one handle, one identity snapshot and one instance
/// configuration.
///
/// Borrowed for the same reason [`Operations`](crate::Operations) is: a
/// request already holds the `Arc<AppData>` it decided under.
///
/// Like `Operations`, this type performs **no authorization of the caller**.
/// The consent operations take the [`Caller`] the host authenticated and bind
/// the grant to them; who was allowed to reach the consent screen at all is
/// the host's middleware decision.
pub struct Issuer<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    config: &'a AppConfig,
}

impl<'a, C> Issuer<'a, C> {
    pub(crate) fn new(gproxy: &'a Gproxy<C>, data: &'a AppData, config: &'a AppConfig) -> Self {
        Self {
            gproxy,
            data,
            config,
        }
    }

    pub fn store(&self) -> &'a Store<C> {
        self.gproxy.store()
    }

    pub fn data(&self) -> &'a AppData {
        self.data
    }

    pub fn config(&self) -> &'a AppConfig {
        self.config
    }

    fn audit(&self) -> Audit<'a, C> {
        Audit::new(self.gproxy.store(), self.config.audit_enabled)
    }

    /// RFC 8414 §2's discovery document for one mount.
    ///
    /// Pure: it embeds `origin` and this crate's fixed capabilities, and reads
    /// nothing. v3 never published this, so every client had gproxy's endpoint
    /// paths compiled in and no way to discover which mount it had been
    /// pointed at.
    pub fn metadata(&self, origin: &IssuerOrigin) -> AuthorizationServerMetadata {
        AuthorizationServerMetadata {
            issuer: origin.issuer(),
            authorization_endpoint: origin.endpoint("/oauth/authorize"),
            token_endpoint: origin.endpoint("/oauth/token"),
            device_authorization_endpoint: origin.endpoint("/oauth/device/code"),
            revocation_endpoint: origin.endpoint("/oauth/revoke"),
            response_types_supported: vec!["code".into()],
            grant_types_supported: vec![
                "authorization_code".into(),
                "refresh_token".into(),
                DEVICE_GRANT_TYPE.into(),
            ],
            code_challenge_methods_supported: vec!["S256".into()],
            token_endpoint_auth_methods_supported: vec!["none".into()],
            revocation_endpoint_auth_methods_supported: vec!["none".into()],
        }
    }
}

impl<C: BatchConnectionTrait> Issuer<'_, C> {
    /// A registered client that can still be authorized against, or
    /// `invalid_client`.
    ///
    /// Read from the database rather than from the snapshot: a client
    /// registered or retired by the operation before this one is not in the
    /// snapshot yet, and being one revision behind must not let a retired
    /// client keep issuing. The snapshot's copy is for listing, not for
    /// deciding.
    async fn client(&self, client_id: &str) -> Result<client::Model> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(IssuerError::invalid_request("client_id is required"));
        }
        self.store()
            .oauth_clients()
            .get_many(&[client_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .filter(|row| row.enabled && row.deleted_at_ms.is_none())
            .ok_or_else(|| {
                IssuerError::invalid_client(format!(
                    "`{client_id}` is not a registered, enabled client"
                ))
            })
    }

    /// That this user may authorize this client at all.
    ///
    /// Checked twice, as everywhere else the client allowlist appears. The
    /// snapshot answers first because it is free, but it cannot see the
    /// **global** level — that lives on the `settings` row, which is in
    /// `ControlData` — so the store's `allowed_many` is the authority, and it
    /// evaluates the same policy the write statements evaluate. A user the
    /// snapshot admits and the store refuses is refused; the reverse cannot
    /// happen without the snapshot being stale, and a stale refusal is a
    /// retry, not a breach.
    async fn allow_client(&self, user_id: &str, client_id: &str) -> Result<()> {
        let organizations = self.data.memberships.effective_organizations(user_id);
        let teams: Vec<String> = self
            .data
            .memberships
            .teams_of(user_id)
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let refuse = || {
            IssuerError::unauthorized_client(format!(
                "`{client_id}` is not available to this account"
            ))
        };
        if !self
            .data
            .oauth_client_allowlist
            .allowed(user_id, &organizations, &teams, client_id)
        {
            return Err(refuse());
        }
        let allowed = self
            .store()
            .oauth_clients()
            .allowed_many(&[gproxy_store::operations::oauth::ClientAccess {
                user_id: user_id.to_owned(),
                client_id: client_id.to_owned(),
            }])
            .await?;
        if allowed.first().copied().unwrap_or(false) {
            Ok(())
        } else {
            Err(refuse())
        }
    }

    /// Append a row to the audit trail, swallowing a failure into a warning.
    ///
    /// Always best-effort here, and that is load-bearing rather than lazy: a
    /// trail write that turned a successful exchange into a 500 would have the
    /// client retry with a code it has already spent, and the replay rule
    /// would then revoke the grant it had just been given.
    async fn record(&self, entry: AuditEntry) {
        self.audit().try_record(entry).await;
    }
}

/// Where this issuer answers, as the host resolved it.
///
/// **Deliberately not computed in this crate.** The same instance serves the
/// issuer at up to three mounts — `https://host`, `https://host/{namespace}/v1`
/// and `https://host/{provider}/v1` — and RFC 8414 §2 requires the `issuer`
/// identifier to be the exact one a client fetched the document from. Only the
/// host knows which mount the request arrived on.
///
/// The scheme is the other half of that. `x-forwarded-proto` is a header any
/// client can send, so a host must believe it **only from a peer listed in
/// [`AppConfig::trusted_proxies`](crate::AppConfig)**, and fall back to
/// [`AppConfig::public_base_url`](crate::AppConfig) or to the socket's own
/// scheme otherwise. An issuer identifier taken from an attacker-supplied
/// `Host` or `x-forwarded-proto` is a discovery document pointing at somebody
/// else's endpoints.
///
/// Two parts, because they are used differently: `origin` is the instance
/// (`https://gproxy.example.com`) and `mount` is the path prefix the issuer is
/// under (`""`, `/acme/v1`). The protocol endpoints hang off the mount, so a
/// client discovers the one it is talking to; the device verification page
/// hangs off the origin, because the portal is one application regardless of
/// how many mounts the data plane answers at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuerOrigin {
    origin: String,
    mount: String,
}

impl IssuerOrigin {
    /// An absolute `http(s)://authority` base, with any path it carries taken
    /// as the initial mount — so
    /// [`AppConfig::public_base_url`](crate::AppConfig) can be passed straight
    /// in even when the instance lives under a path.
    ///
    /// A failure here is a host configuration error, not a client request
    /// error, so it is [`AppError::Invalid`] and not an OAuth code: no client
    /// caused it and none can fix it.
    pub fn new(base: &str) -> Result<Self> {
        let base = base.trim().trim_end_matches('/');
        let (scheme, rest) = base
            .split_once("://")
            .ok_or_else(|| AppError::invalid(format!("`{base}` is not an absolute origin")))?;
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return Err(AppError::invalid(format!("`{base}` must be http or https")));
        }
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        if authority.is_empty()
            || authority.contains('@')
            || authority.contains(char::is_whitespace)
        {
            return Err(AppError::invalid(format!(
                "`{base}` must carry a host and no userinfo"
            )));
        }
        if rest.contains('?') || rest.contains('#') {
            return Err(AppError::invalid(format!(
                "`{base}` must not carry a query or a fragment"
            )));
        }
        Self {
            origin: format!("{}://{authority}", scheme.to_ascii_lowercase()),
            mount: String::new(),
        }
        .mounted_at(path)
    }

    /// The same origin, with the issuer mounted under `mount`. An empty or
    /// `/` mount is the instance root.
    pub fn mounted_at(mut self, mount: &str) -> Result<Self> {
        let mount = mount.trim().trim_end_matches('/');
        if mount.is_empty() {
            self.mount = String::new();
            return Ok(self);
        }
        if !mount.starts_with('/') || mount.contains('?') || mount.contains('#') {
            return Err(AppError::invalid(format!(
                "`{mount}` must be an absolute path with no query or fragment"
            )));
        }
        self.mount = mount.to_owned();
        Ok(self)
    }

    /// `https://gproxy.example.com`, without the mount.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The RFC 8414 issuer identifier: origin and mount, no trailing slash.
    pub fn issuer(&self) -> String {
        format!("{}{}", self.origin, self.mount)
    }

    /// A protocol endpoint, under the mount.
    pub fn endpoint(&self, path: &str) -> String {
        format!("{}{}{path}", self.origin, self.mount)
    }

    /// An instance page, at the root and never under the mount.
    pub fn instance(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }
}

/// 32 bytes of entropy as a bearer value, with the digest its row keeps.
///
/// The same shape as [`generate_api_key`](crate::auth::generate_api_key) and
/// as a console session token, and for the same reason: one answer in this
/// crate to "how much entropy is a credential, and how is it stored". The
/// digest is raw bytes rather than hex because `oauth_tokens.token_hash`,
/// `oauth_codes.code_hash` and `oauth_devices.device_code_hash` are
/// `Binary(32)` columns, where `api_keys.key_hash` is text; the hash function
/// and the input are identical, so [`token_digest`] serves both.
fn mint_secret() -> Result<(String, Vec<u8>)> {
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    let digest = token_digest(&secret).to_vec();
    Ok((secret, digest))
}

/// Seconds of configured lifetime as an absolute millisecond instant.
fn expires_at(now_ms: i64, ttl_secs: u64) -> i64 {
    now_ms.saturating_add(
        i64::try_from(ttl_secs)
            .unwrap_or(i64::MAX)
            .saturating_mul(1_000),
    )
}

/// RFC 6749 §3.3's scope list: space-delimited, order preserved, duplicates
/// dropped.
///
/// A scope token is `1*( %x21 / %x23-5B / %x5D-7E )` — printable ASCII except
/// space, `"` and `\`. Anything else is `invalid_scope` rather than silently
/// dropped: a client that asked for something this issuer cannot spell back
/// deserves to know, and a scope stored with a quote in it would not survive a
/// round trip through the space-delimited wire form.
fn parse_scopes(scope: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for token in scope.split_whitespace() {
        if !token
            .bytes()
            .all(|byte| (0x21..=0x7E).contains(&byte) && byte != b'"' && byte != b'\\')
        {
            return Err(IssuerError::invalid_scope(format!(
                "`{token}` is not a valid scope token"
            )));
        }
        if !out.iter().any(|seen| seen == token) {
            out.push(token.to_owned());
        }
    }
    Ok(out)
}

/// The wire form of a granted scope list.
fn join_scopes(scopes: &[String]) -> String {
    scopes.join(" ")
}

/// `oauth_grants.scopes` / `oauth_devices.scopes` as a list.
///
/// Accepts the array this issuer writes and the space-delimited string that is
/// the wire form, for the same reason
/// [`auth::oauth_access`](crate::auth) does: a row written by hand is not
/// worth refusing a live grant over. Anything else grants nothing.
fn stored_scopes(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str())
            .map(str::to_owned)
            .collect(),
        Value::String(text) => text.split_whitespace().map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// RFC 7636 §4.6's S256 transformation: base64url, unpadded, of the SHA-256 of
/// the verifier's **ASCII** bytes.
fn s256(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The row behind a `Binary(32)` digest of one of the OAuth tables, or None.
///
/// A digest of the wrong length can never match a row, so it is a miss rather
/// than an error: the caller turns every miss into the same `invalid_grant`,
/// and distinguishing "malformed" from "unknown" here would tell a prober
/// which of its guesses had the right shape.
async fn by_digest<C, E>(
    repository: gproxy_store::Repository<'_, C, E>,
    column: E::Column,
    digest: &[u8],
) -> Result<Option<E::Model>>
where
    C: BatchConnectionTrait,
    E: EntityTrait,
    gproxy_store::Key<E>: Clone + Eq + std::hash::Hash + Sync,
{
    if digest.len() != 32 {
        return Ok(None);
    }
    Ok(repository
        .query(E::find().filter(column.eq(digest.to_vec())))
        .await?
        .into_iter()
        .next())
}

/// The actor columns of an audit row for an operation a *client* performed,
/// where there is no authenticated [`Caller`] at all — only a grant that names
/// a user.
fn by_grant(entry: AuditEntry, user_id: &str, api_key_id: &str) -> AuditEntry {
    AuditEntry {
        actor_user_id: Some(user_id.to_owned()),
        actor_api_key_id: Some(api_key_id.to_owned()),
        ..entry
    }
}

/// A caller's own audit actor columns, for the consent operations, which do
/// run as a person.
fn by_caller(entry: AuditEntry, caller: &Caller) -> AuditEntry {
    entry.by(caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_keeps_its_path_as_the_mount() {
        let origin = IssuerOrigin::new("https://gproxy.example.com").unwrap();
        assert_eq!(origin.issuer(), "https://gproxy.example.com");
        assert_eq!(
            origin.endpoint("/oauth/token"),
            "https://gproxy.example.com/oauth/token"
        );
        assert_eq!(
            origin.instance("/portal/device"),
            "https://gproxy.example.com/portal/device"
        );

        let mounted = IssuerOrigin::new("https://gproxy.example.com/acme/v1/").unwrap();
        assert_eq!(mounted.issuer(), "https://gproxy.example.com/acme/v1");
        assert_eq!(
            mounted.endpoint("/oauth/token"),
            "https://gproxy.example.com/acme/v1/oauth/token"
        );
        // The portal is at the root however the data plane is mounted.
        assert_eq!(
            mounted.instance("/portal/device"),
            "https://gproxy.example.com/portal/device"
        );
    }

    #[test]
    fn a_mount_can_be_set_after_the_origin_is_known() {
        let origin = IssuerOrigin::new("https://gproxy.example.com")
            .unwrap()
            .mounted_at("/openai/v1")
            .unwrap();
        assert_eq!(origin.issuer(), "https://gproxy.example.com/openai/v1");
        // And cleared again.
        assert_eq!(
            origin.mounted_at("/").unwrap().issuer(),
            "https://gproxy.example.com"
        );
    }

    #[test]
    fn a_base_that_could_point_somewhere_else_is_refused() {
        for bad in [
            "gproxy.example.com",
            "ftp://gproxy.example.com",
            "https://",
            "https://user:pass@evil.example",
            "https://gproxy.example.com/v1?x=1",
            "https://gproxy.example.com/v1#f",
            "https://gproxy example.com",
        ] {
            let error = IssuerOrigin::new(bad).unwrap_err();
            assert_eq!(error.status_code(), 400, "{bad} was accepted");
        }
        assert!(
            IssuerOrigin::new("https://h")
                .unwrap()
                .mounted_at("relative")
                .is_err()
        );
    }

    #[test]
    fn the_scheme_is_normalized_but_the_host_is_left_alone() {
        // A host may be case-sensitive in a way this layer cannot know.
        let origin = IssuerOrigin::new("HTTPS://GProxy.Example.com").unwrap();
        assert_eq!(origin.origin(), "https://GProxy.Example.com");
    }

    #[test]
    fn scopes_are_deduplicated_in_the_order_they_were_asked_for() {
        assert_eq!(
            parse_scopes("openid profile openid  offline_access").unwrap(),
            ["openid", "profile", "offline_access"]
        );
        assert!(parse_scopes("   ").unwrap().is_empty());
        assert_eq!(join_scopes(&parse_scopes("a b").unwrap()), "a b");
    }

    #[test]
    fn a_scope_that_cannot_survive_the_wire_form_is_refused() {
        for bad in ["a\"b", "a\\b", "a\u{7f}b", "héllo"] {
            let error = parse_scopes(bad).unwrap_err();
            assert_eq!(error.code(), "invalid_scope", "{bad} was accepted");
        }
    }

    #[test]
    fn stored_scopes_read_an_array_or_the_wire_string() {
        assert_eq!(
            stored_scopes(&serde_json::json!(["a", "b"])),
            ["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            stored_scopes(&serde_json::json!("a b")),
            ["a".to_string(), "b".to_string()]
        );
        assert!(stored_scopes(&serde_json::json!(7)).is_empty());
    }

    #[test]
    fn s256_is_the_rfc_7636_test_vector() {
        // RFC 7636 appendix B.
        assert_eq!(
            s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_minted_secret_is_32_bytes_stored_only_as_its_digest() {
        let (secret, digest) = mint_secret().unwrap();
        assert_eq!(secret.len(), 43);
        assert!(!secret.contains('='));
        assert_eq!(digest.len(), 32);
        assert_eq!(digest, token_digest(&secret).to_vec());
        // And two are not the same secret.
        assert_ne!(mint_secret().unwrap().0, secret);
    }

    #[test]
    fn a_device_code_can_never_be_redeemed_as_an_authorization_code() {
        // The registry refuses to store a redirect URI without `://`, so this
        // sentinel cannot be registered and cannot be presented.
        assert!(!DEVICE_REDIRECT_URI.contains("://"));
    }

    #[test]
    fn a_lifetime_saturates_rather_than_wrapping() {
        assert_eq!(expires_at(1_000, 1), 2_000);
        assert_eq!(expires_at(i64::MAX - 1, 3600), i64::MAX);
    }
}
