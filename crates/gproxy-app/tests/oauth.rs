#![cfg(not(target_arch = "wasm32"))]
//! The OAuth issuer against a real database.
//!
//! Everything here drives the same code a host would: a `Gproxy` handle over
//! in-memory SQLite, the P6 operation families for seeding, and the real
//! [`Authenticator`] to prove that what the issuer minted actually
//! authenticates. Nothing reaches a network and nothing stubs the store — the
//! atomic batches (`issue_many`, `exchange_tokens_many`, `revoke_many`) are
//! the part that has to be right, and a fake would be testing the fake.
//!
//! The properties asserted over and over:
//!
//! 1. a one-shot credential works **once**;
//! 2. presenting a spent refresh token or code kills the **whole** grant, the
//!    already-issued access token included;
//! 3. tokens exist only in the response that minted them — every row holds a
//!    digest;
//! 4. issuing and revoking do **not** move `settings.config_revision`.

use gproxy_app::{
    AppConfig, AppData, Authenticator, Caller, CallerKind, IssuerOrigin, Operations,
    dto::{
        AuditQuery, AuthorizeOutcome, AuthorizeQuery, ConsentDecision, DEVICE_GRANT_TYPE,
        DeviceCodeRequest, OAuthClientWrite, RevokeRequest, TokenRequest, UserPatch, UserWrite,
    },
};
use gproxy_cache::{MemoryCache, MemoryOptions};
use gproxy_sdk::{Gproxy, GproxyBuilder, SyncMode};
use gproxy_store::{
    Store,
    entity::{
        identity::api_key,
        oauth::{code, device, grant, token},
    },
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use std::sync::Arc;

/// RFC 7636 appendix B's verifier and its S256 challenge, so the PKCE checks
/// are asserted against the specification's own vector rather than against
/// this implementation's output.
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

const REDIRECT: &str = "http://127.0.0.1:1455/auth/callback";

/// A handle over a private in-memory database, with no background poller
/// deciding when a snapshot advances.
async fn handle() -> Gproxy<DatabaseConnection> {
    GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .cache(Arc::new(
            MemoryCache::new(MemoryOptions::default()).unwrap(),
        ))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap()
}

async fn snapshot(store: &Store<DatabaseConnection>) -> AppData {
    let all = store.load_all_data().await.unwrap();
    AppData::assemble(
        revision(store).await,
        &all.identity,
        &all.control.credentials,
    )
    .unwrap()
}

async fn revision(store: &Store<DatabaseConnection>) -> i64 {
    store
        .settings()
        .get()
        .await
        .unwrap()
        .expect("the builder creates the settings row")
        .config_revision
}

/// One user and one registered client, seeded through the P6 families so the
/// rows are exactly what the management API would have written.
async fn seed(gproxy: &Gproxy<DatabaseConnection>) -> (String, String) {
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let operations = Operations::new(gproxy, &data, &config);
    let user = operations
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    let client = operations
        .oauth_clients()
        .create(OAuthClientWrite {
            id: "cli-app".into(),
            name: "CLI".into(),
            redirect_uris: vec![REDIRECT.into()],
            ..OAuthClientWrite::default()
        })
        .await
        .unwrap();
    (user.id, client.id)
}

/// The portal session that drives a consent screen: a person, with no key and
/// therefore no organization, team or subscription binding.
fn person(user_id: &str) -> Caller {
    Caller {
        user_id: user_id.to_owned(),
        user_role: "user".into(),
        api_key_id: None,
        organization_id: None,
        team_id: None,
        subscription_id: None,
        grant: None,
        kind: CallerKind::Session,
    }
}

fn authorize_query(client_id: &str) -> AuthorizeQuery {
    AuthorizeQuery {
        response_type: "code".into(),
        client_id: client_id.into(),
        redirect_uri: REDIRECT.into(),
        scope: "openid profile".into(),
        state: Some("xyz".into()),
        code_challenge: CHALLENGE.into(),
        code_challenge_method: "S256".into(),
    }
}

fn code_exchange(client_id: &str, code: &str) -> TokenRequest {
    TokenRequest {
        grant_type: "authorization_code".into(),
        client_id: client_id.into(),
        code: Some(code.into()),
        redirect_uri: Some(REDIRECT.into()),
        code_verifier: Some(VERIFIER.into()),
        ..TokenRequest::default()
    }
}

fn refresh_exchange(client_id: &str, refresh_token: &str) -> TokenRequest {
    TokenRequest {
        grant_type: "refresh_token".into(),
        client_id: client_id.into(),
        refresh_token: Some(refresh_token.into()),
        ..TokenRequest::default()
    }
}

/// Approve an authorization and answer the code.
async fn approve(gproxy: &Gproxy<DatabaseConnection>, user_id: &str, client_id: &str) -> String {
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let outcome = Operations::new(gproxy, &data, &config)
        .issuer()
        .authorize(
            &person(user_id),
            &authorize_query(client_id),
            ConsentDecision::Approve,
        )
        .await
        .unwrap();
    match outcome {
        AuthorizeOutcome::Issued(issued) => {
            assert_eq!(issued.redirect_uri, REDIRECT);
            assert_eq!(issued.state.as_deref(), Some("xyz"));
            issued.code
        }
        AuthorizeOutcome::Denied(denied) => panic!("expected a code, got {denied:?}"),
    }
}

/// Whether a token still authenticates, at the current clock.
async fn authenticates(gproxy: &Gproxy<DatabaseConnection>, token: &str) -> bool {
    authenticates_at(gproxy, token, gproxy_now()).await
}

/// The same at a stated clock, for the device tests, which run on a fake one:
/// a token minted at 2 000 ms expires long before wall-clock now.
async fn authenticates_at(gproxy: &Gproxy<DatabaseConnection>, token: &str, now_ms: i64) -> bool {
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    Authenticator::new(gproxy.store(), &data, &config)
        .authenticate_token_at(token, now_ms)
        .await
        .is_ok()
}

fn gproxy_now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// The authorization code flow.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_code_flow_issues_a_token_that_authenticates_as_its_grant() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let operations = Operations::new(&gproxy, &data, &config);
    let issuer = operations.issuer();

    // The consent screen sees the client's display name and the scopes asked
    // for, already split.
    let details = issuer
        .authorize_details(&person(&user_id), &authorize_query(&client_id))
        .await
        .unwrap();
    assert_eq!(details.client_name, "CLI");
    assert_eq!(details.scopes, ["openid", "profile"]);
    assert_eq!(details.redirect_uri, REDIRECT);
    assert_eq!(details.user_id, user_id);

    let before = revision(gproxy.store()).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let tokens = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();
    assert_eq!(tokens.token_type, "Bearer");
    assert_eq!(tokens.expires_in, 3600);
    assert_eq!(tokens.scope, "openid profile");
    assert!(!tokens.access_token.is_empty());
    assert_ne!(tokens.access_token, tokens.refresh_token);

    // Issuing is not a configuration write: a refresh must not make every peer
    // in the fleet reload.
    assert_eq!(revision(gproxy.store()).await, before);

    // The access token authenticates, and does so *as the grant* rather than
    // as an ordinary key.
    let data = snapshot(gproxy.store()).await;
    let caller = Authenticator::new(gproxy.store(), &data, &config)
        .authenticate_token(&tokens.access_token)
        .await
        .unwrap();
    assert_eq!(caller.kind, CallerKind::OAuthGrant);
    assert_eq!(caller.user_id, user_id);
    let grant = caller.grant.expect("an OAuth caller carries its grant");
    assert_eq!(grant.client_id, client_id);
    assert_eq!(grant.scopes, ["openid", "profile"]);

    // The grant's internal key exists, is of the OAuth kind, and is refused
    // when it is presented as a bearer key — which is why its plaintext is
    // never returned anywhere.
    let key = api_key::Entity::find_by_id(caller.api_key_id.clone().unwrap())
        .one(gproxy.store().connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(key.kind, api_key::ApiKeyKind::OAuth);
    assert_eq!(key.user_id, user_id);
    assert!(key.secret.is_none());

    // No row anywhere holds a token's plaintext.
    let rows = token::Entity::find()
        .all(gproxy.store().connection())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.token_hash.len(), 32);
        assert!(!String::from_utf8_lossy(&row.token_hash).contains(&tokens.access_token));
    }
    // The refresh token works once and yields a different pair.
    let rotated = issuer
        .token(&refresh_exchange(&client_id, &tokens.refresh_token))
        .await
        .unwrap();
    assert_ne!(rotated.refresh_token, tokens.refresh_token);
    assert_ne!(rotated.access_token, tokens.access_token);
    assert!(authenticates(&gproxy, &rotated.access_token).await);
}

#[tokio::test]
async fn a_wrong_code_verifier_does_not_spend_the_code() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let mut request = code_exchange(&client_id, &code);
    request.code_verifier = Some("Zm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm9vYmFyZm8".into());
    let error = issuer.token(&request).await.unwrap_err();
    assert_eq!(error.code(), "invalid_grant");
    assert_eq!(error.status_code(), 400);

    // Refusing a wrong verifier must not consume the code: the legitimate
    // client is still holding it.
    let tokens = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();
    assert!(authenticates(&gproxy, &tokens.access_token).await);
}

#[tokio::test]
async fn a_redirect_uri_that_is_only_a_prefix_is_refused() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    for near in [
        // Each of these is what a prefix, suffix or normalization rule would
        // have let through, and each is a URL somebody else can control.
        "http://127.0.0.1:1455/auth/callback/../../elsewhere",
        "http://127.0.0.1:1455/auth/callback.attacker.example",
        "http://127.0.0.1:1455/auth/callback?next=//evil.example",
        "http://127.0.0.1:1455/auth/callbac",
        "http://127.0.0.1:1455/auth/callback/",
        "https://127.0.0.1:1455/auth/callback",
    ] {
        let mut query = authorize_query(&client_id);
        query.redirect_uri = near.into();
        let error = issuer
            .authorize_details(&person(&user_id), &query)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request", "{near} was accepted");
        // And the approval refuses it too, rather than relying on the screen.
        let error = issuer
            .authorize(&person(&user_id), &query, ConsentDecision::Approve)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request", "{near} was approved");
    }
    assert_eq!(
        grant::Entity::find()
            .all(gproxy.store().connection())
            .await
            .unwrap()
            .len(),
        0,
        "a refused authorization wrote a grant"
    );
}

#[tokio::test]
async fn pkce_is_s256_or_nothing() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    // `plain` puts the verifier in the authorization request itself, where the
    // browser, every proxy and every log along the redirect can read it.
    for method in ["plain", "", "S512", "S256plain", "sha256"] {
        let mut query = authorize_query(&client_id);
        query.code_challenge_method = method.into();
        let error = issuer
            .authorize_details(&person(&user_id), &query)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_request", "`{method}` was accepted");
    }
    // The spelling itself is read leniently: a form value with surrounding
    // whitespace, or a client that lowercased it, still names the one
    // transformation this issuer performs.
    for method in ["S256", "s256", " S256 "] {
        let mut query = authorize_query(&client_id);
        query.code_challenge_method = method.into();
        assert!(
            issuer
                .authorize_details(&person(&user_id), &query)
                .await
                .is_ok(),
            "`{method}` was refused"
        );
    }
    // A missing challenge is refused even with the right method.
    let mut query = authorize_query(&client_id);
    query.code_challenge = String::new();
    assert!(
        issuer
            .authorize_details(&person(&user_id), &query)
            .await
            .is_err()
    );
    // And so is a response type this issuer has no flow for.
    let mut query = authorize_query(&client_id);
    query.response_type = "token".into();
    let error = issuer
        .authorize_details(&person(&user_id), &query)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_response_type");
}

#[tokio::test]
async fn a_replayed_code_revokes_the_grant_it_created() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let tokens = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();
    assert!(authenticates(&gproxy, &tokens.access_token).await);

    // RFC 6749 §4.1.2: a code presented twice has leaked, so everything issued
    // under it goes.
    let error = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_grant");
    assert!(
        !authenticates(&gproxy, &tokens.access_token).await,
        "the access token survived the replay"
    );
    assert!(
        issuer
            .token(&refresh_exchange(&client_id, &tokens.refresh_token))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_replayed_refresh_token_takes_the_whole_family_with_it() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let first = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();
    let second = issuer
        .token(&refresh_exchange(&client_id, &first.refresh_token))
        .await
        .unwrap();
    assert!(authenticates(&gproxy, &second.access_token).await);

    // The rotated-away refresh token is presented again. Either the client has
    // it twice or somebody else does, and from here the two are the same
    // event.
    let error = issuer
        .token(&refresh_exchange(&client_id, &first.refresh_token))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_grant");

    // The whole family is dead: the current access token, the current refresh
    // token, and the grant's internal key.
    assert!(
        !authenticates(&gproxy, &second.access_token).await,
        "the live access token survived the replay"
    );
    assert!(!authenticates(&gproxy, &first.access_token).await);
    assert!(
        issuer
            .token(&refresh_exchange(&client_id, &second.refresh_token))
            .await
            .is_err()
    );
    let connection = gproxy.store().connection();
    let row = grant::Entity::find()
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    assert!(row.revoked_at_ms.is_some(), "the grant was not revoked");
    assert_eq!(row.refresh_count, 1);
    let key = api_key::Entity::find_by_id(row.api_key_id)
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    assert!(!key.enabled, "the grant's internal key is still enabled");
    for issued in token::Entity::find().all(connection).await.unwrap() {
        assert!(issued.revoked_at_ms.is_some(), "{issued:?} survived");
    }
}

#[tokio::test]
async fn a_token_request_names_what_it_is_missing() {
    let gproxy = handle().await;
    let (_, client_id) = seed(&gproxy).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let error = issuer.token(&TokenRequest::default()).await.unwrap_err();
    assert_eq!(error.code(), "invalid_request");
    let error = issuer
        .token(&TokenRequest {
            grant_type: "password".into(),
            client_id: client_id.clone(),
            ..TokenRequest::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unsupported_grant_type");
    // An unknown client is `invalid_client`, not `invalid_grant`: the request
    // never got as far as a credential.
    let error = issuer
        .token(&code_exchange("nobody", "c"))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_client");
    // A well-formed request for a code that does not exist.
    let error = issuer
        .token(&code_exchange(&client_id, "not-a-code"))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_grant");
}

// ---------------------------------------------------------------------------
// The device flow.
// ---------------------------------------------------------------------------

/// A device authorization at a stated clock, answering the poll request the
/// device would build.
async fn device_start(
    gproxy: &Gproxy<DatabaseConnection>,
    client_id: &str,
    now_ms: i64,
) -> (String, String, TokenRequest) {
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let origin = IssuerOrigin::new("https://gproxy.example.com").unwrap();
    let started = Operations::new(gproxy, &data, &config)
        .issuer()
        .device_code_at(
            &DeviceCodeRequest {
                client_id: client_id.into(),
                scope: "openid".into(),
            },
            &origin,
            now_ms,
        )
        .await
        .unwrap();
    assert_eq!(
        started.verification_uri,
        "https://gproxy.example.com/portal/device"
    );
    assert_eq!(
        started.verification_uri_complete,
        format!(
            "https://gproxy.example.com/portal/device?user_code={}",
            started.user_code
        )
    );
    assert_eq!(started.expires_in, 900);
    assert_eq!(started.interval, 5);
    // Grouped for a person to read, nine characters including the separator.
    assert_eq!(started.user_code.len(), 9);
    let poll = TokenRequest {
        grant_type: DEVICE_GRANT_TYPE.into(),
        client_id: client_id.into(),
        device_code: Some(started.device_code.clone()),
        ..TokenRequest::default()
    };
    (started.device_code, started.user_code, poll)
}

#[tokio::test]
async fn the_device_flow_polls_until_it_is_approved() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let (device_code, user_code, poll) = device_start(&gproxy, &client_id, 0).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    // Before a decision, and with a code typed in every spelling a person
    // might use.
    let error = issuer.token_at(&poll, 1_000).await.unwrap_err();
    assert_eq!(error.code(), "authorization_pending");
    assert_eq!(error.status_code(), 400);
    for spelling in [
        user_code.clone(),
        user_code.to_lowercase(),
        user_code.replace('-', ""),
        format!(" {user_code} "),
    ] {
        let details = issuer.device_details_at(&spelling, 1_000).await.unwrap();
        assert_eq!(details.client_name, "CLI");
        assert_eq!(details.scopes, ["openid"]);
        assert_eq!(details.user_code, user_code.replace('-', ""));
    }

    issuer
        .device_decision_at(
            &person(&user_id),
            &user_code,
            ConsentDecision::Approve,
            1_000,
        )
        .await
        .unwrap();
    let tokens = issuer.token_at(&poll, 2_000).await.unwrap();
    assert_eq!(tokens.scope, "openid");
    assert!(authenticates_at(&gproxy, &tokens.access_token, 3_000).await);

    // A second poll is refused — the device code is spent — but it does
    // **not** revoke the family, because a polling client legitimately
    // re-sends the same code when a response is lost.
    let error = issuer.token_at(&poll, 3_000).await.unwrap_err();
    assert_eq!(error.code(), "invalid_grant");
    assert!(
        authenticates_at(&gproxy, &tokens.access_token, 4_000).await,
        "a repeated device poll revoked the tokens it had just issued"
    );
    // And the device row records that it was consumed.
    let row = device::Entity::find()
        .one(gproxy.store().connection())
        .await
        .unwrap()
        .unwrap();
    assert!(row.consumed_at_ms.is_some());
    assert_eq!(row.device_code_hash.len(), 32);
    assert!(!String::from_utf8_lossy(&row.device_code_hash).contains(&device_code));
    // The code minted for a device carries a redirect no registration can
    // hold, so it can never be redeemed through the browser grant type.
    let authorization = code::Entity::find()
        .one(gproxy.store().connection())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(authorization.redirect_uri, DEVICE_GRANT_TYPE);
}

#[tokio::test]
async fn a_denied_device_authorization_answers_access_denied() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let (_, user_code, poll) = device_start(&gproxy, &client_id, 0).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let decided = issuer
        .device_decision_at(&person(&user_id), &user_code, ConsentDecision::Deny, 1_000)
        .await
        .unwrap();
    assert!(!decided.approved);
    let error = issuer.token_at(&poll, 2_000).await.unwrap_err();
    assert_eq!(error.code(), "access_denied");
    assert_eq!(error.status_code(), 403);

    // A decided authorization cannot be decided again, in either direction.
    for decision in [ConsentDecision::Approve, ConsentDecision::Deny] {
        assert!(
            issuer
                .device_decision_at(&person(&user_id), &user_code, decision, 2_000)
                .await
                .is_err()
        );
    }
    assert_eq!(
        grant::Entity::find()
            .all(gproxy.store().connection())
            .await
            .unwrap()
            .len(),
        0,
        "a denial created a grant"
    );
}

#[tokio::test]
async fn an_expired_device_code_says_so_rather_than_pending() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let (_, user_code, poll) = device_start(&gproxy, &client_id, 0).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    // The default lifetime is 900 seconds, and the boundary is exclusive.
    assert_eq!(
        issuer.token_at(&poll, 899_999).await.unwrap_err().code(),
        "authorization_pending"
    );
    let error = issuer.token_at(&poll, 900_000).await.unwrap_err();
    assert_eq!(error.code(), "expired_token");
    // The approval page says the same thing, so a person who walks over five
    // minutes late is not told the code never existed.
    let error = issuer
        .device_details_at(&user_code, 900_000)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "expired_token");
    let error = issuer
        .device_decision_at(
            &person(&user_id),
            &user_code,
            ConsentDecision::Approve,
            900_000,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "expired_token");
}

#[tokio::test]
async fn cancelling_a_device_authorization_retires_its_code() {
    let gproxy = handle().await;
    let (_, client_id) = seed(&gproxy).await;
    let (device_code, user_code, poll) = device_start(&gproxy, &client_id, 0).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let cancelled = issuer
        .device_cancel_at(&client_id, &device_code, 1_000)
        .await
        .unwrap();
    assert_eq!(cancelled.user_code, user_code.replace('-', ""));
    let error = issuer.token_at(&poll, 2_000).await.unwrap_err();
    assert_eq!(error.code(), "access_denied");

    // An unknown code is not an error: the outcome asked for is "this code no
    // longer works", and an unknown one already does not.
    let cancelled = issuer
        .device_cancel_at(&client_id, "not-a-device-code", 2_000)
        .await
        .unwrap();
    assert!(cancelled.user_code.is_empty());
}

// ---------------------------------------------------------------------------
// Revocation, the allowlist and retirement.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoking_is_idempotent_grant_wide_and_never_an_oracle() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();
    let tokens = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();

    // RFC 7009 §2.2: an invalid token is not an error.
    assert!(
        !issuer
            .revoke(&RevokeRequest {
                token: "never-issued".into(),
                ..RevokeRequest::default()
            })
            .await
            .unwrap()
    );
    // Another client cannot revoke this grant by naming itself.
    assert!(
        !issuer
            .revoke(&RevokeRequest {
                token: tokens.access_token.clone(),
                client_id: Some("someone-else".into()),
                ..RevokeRequest::default()
            })
            .await
            .unwrap()
    );
    assert!(authenticates(&gproxy, &tokens.access_token).await);

    // Revoking the access token takes the refresh token with it: a grant is
    // the session, and its tokens are rotations of one credential.
    let before = revision(gproxy.store()).await;
    assert!(
        issuer
            .revoke(&RevokeRequest {
                token: tokens.access_token.clone(),
                token_type_hint: Some("access_token".into()),
                client_id: Some(client_id.clone()),
            })
            .await
            .unwrap()
    );
    assert!(!authenticates(&gproxy, &tokens.access_token).await);
    assert!(
        issuer
            .token(&refresh_exchange(&client_id, &tokens.refresh_token))
            .await
            .is_err()
    );
    // Revoking again changes nothing and is still not an error.
    assert!(
        !issuer
            .revoke(&RevokeRequest {
                token: tokens.refresh_token.clone(),
                ..RevokeRequest::default()
            })
            .await
            .unwrap()
    );
    // And none of it moved the configuration revision.
    assert_eq!(revision(gproxy.store()).await, before);
    // A blank token is a malformed request rather than a silent success.
    assert_eq!(
        issuer
            .revoke(&RevokeRequest::default())
            .await
            .unwrap_err()
            .code(),
        "invalid_request"
    );
}

#[tokio::test]
async fn a_client_the_allowlist_forbids_never_reaches_a_consent_screen() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    {
        let data = snapshot(gproxy.store()).await;
        let config = AppConfig::default();
        Operations::new(&gproxy, &data, &config)
            .users()
            .update(
                &user_id,
                UserPatch {
                    oauth_client_allowlist: Some(Some(vec!["some-other-client".into()])),
                    ..UserPatch::default()
                },
            )
            .await
            .unwrap();
    }
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let error = issuer
        .authorize_details(&person(&user_id), &authorize_query(&client_id))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unauthorized_client");
    // The write path refuses it too — the snapshot is a fast path, not the
    // authority.
    let error = issuer
        .authorize(
            &person(&user_id),
            &authorize_query(&client_id),
            ConsentDecision::Approve,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unauthorized_client");
    // And so does the device approval page.
    let (_, user_code, _) = device_start(&gproxy, &client_id, 0).await;
    let error = issuer
        .device_decision_at(
            &person(&user_id),
            &user_code,
            ConsentDecision::Approve,
            1_000,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "unauthorized_client");
    assert_eq!(
        grant::Entity::find()
            .all(gproxy.store().connection())
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn a_denied_consent_writes_nothing_and_redirects_with_the_state() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let outcome = issuer
        .authorize(
            &person(&user_id),
            &authorize_query(&client_id),
            ConsentDecision::Deny,
        )
        .await
        .unwrap();
    let AuthorizeOutcome::Denied(denied) = outcome else {
        panic!("a denial issued a code");
    };
    assert_eq!(denied.error, "access_denied");
    assert_eq!(denied.state.as_deref(), Some("xyz"));
    assert_eq!(denied.redirect_uri, REDIRECT);
    assert!(denied.location().starts_with(&format!(
        "{REDIRECT}?error=access_denied&error_description="
    )));
    assert!(denied.location().ends_with("&state=xyz"));
    let connection = gproxy.store().connection();
    assert!(
        grant::Entity::find()
            .all(connection)
            .await
            .unwrap()
            .is_empty(),
        "a denial created a grant"
    );
    assert!(
        code::Entity::find()
            .all(connection)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        api_key::Entity::find()
            .filter(api_key::Column::Kind.eq(api_key::ApiKeyKind::OAuth))
            .all(connection)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn retiring_a_client_kills_the_tokens_it_had_already_issued() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let tokens = {
        let data = snapshot(gproxy.store()).await;
        let config = AppConfig::default();
        Operations::new(&gproxy, &data, &config)
            .issuer()
            .token(&code_exchange(&client_id, &code))
            .await
            .unwrap()
    };
    assert!(authenticates(&gproxy, &tokens.access_token).await);

    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    Operations::new(&gproxy, &data, &config)
        .oauth_clients()
        .retire(&client_id)
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let issuer = Operations::new(&gproxy, &data, &config).issuer();
    assert!(!authenticates(&gproxy, &tokens.access_token).await);
    // A retired client is no longer a client at all.
    let error = issuer
        .token(&refresh_exchange(&client_id, &tokens.refresh_token))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_client");
    let error = issuer
        .authorize_details(&person(&user_id), &authorize_query(&client_id))
        .await
        .unwrap_err();
    assert_eq!(error.code(), "invalid_client");
}

// ---------------------------------------------------------------------------
// The trail.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_issuance_refusal_and_revocation_reaches_the_audit_trail() {
    let gproxy = handle().await;
    let (user_id, client_id) = seed(&gproxy).await;
    let code = approve(&gproxy, &user_id, &client_id).await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let operations = Operations::new(&gproxy, &data, &config);
    let issuer = operations.issuer();

    let tokens = issuer
        .token(&code_exchange(&client_id, &code))
        .await
        .unwrap();
    let rotated = issuer
        .token(&refresh_exchange(&client_id, &tokens.refresh_token))
        .await
        .unwrap();
    // A replay, which both refuses and revokes.
    assert!(
        issuer
            .token(&refresh_exchange(&client_id, &tokens.refresh_token))
            .await
            .is_err()
    );

    let page = operations
        .audit()
        .query(AuditQuery {
            page_size: Some(50),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    let actions: Vec<&str> = page.items.iter().map(|row| row.action.as_str()).collect();
    for expected in [
        "oauth.authorize.approve",
        "oauth.token.code",
        "oauth.token.refresh",
    ] {
        assert!(
            actions.contains(&expected),
            "{expected} missing: {actions:?}"
        );
    }
    let replay = page
        .items
        .iter()
        .find(|row| row.action == "oauth.token.refresh" && row.outcome == "error")
        .expect("the replay was not recorded");
    assert_eq!(replay.detail["replay"], serde_json::json!(true));
    assert_eq!(replay.detail["error"], serde_json::json!("invalid_grant"));
    assert_eq!(replay.actor_user_id.as_deref(), Some(user_id.as_str()));

    // No row quotes a token. The trail redacts by field name before it is
    // stored, and nothing here puts one in a detail to begin with.
    let text = serde_json::to_string(&page.items).unwrap();
    for secret in [
        &code,
        &tokens.access_token,
        &tokens.refresh_token,
        &rotated.access_token,
    ] {
        assert!(
            !text.contains(secret.as_str()),
            "a secret reached the trail"
        );
    }
}

// ---------------------------------------------------------------------------
// Discovery.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn the_discovery_document_describes_the_mount_it_was_fetched_from() {
    let gproxy = handle().await;
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let issuer = Operations::new(&gproxy, &data, &config).issuer();

    let metadata = issuer.metadata(&IssuerOrigin::new("https://gproxy.example.com").unwrap());
    assert_eq!(metadata.issuer, "https://gproxy.example.com");
    assert_eq!(
        metadata.authorization_endpoint,
        "https://gproxy.example.com/oauth/authorize"
    );
    assert_eq!(
        metadata.token_endpoint,
        "https://gproxy.example.com/oauth/token"
    );
    assert_eq!(
        metadata.device_authorization_endpoint,
        "https://gproxy.example.com/oauth/device/code"
    );
    assert_eq!(
        metadata.revocation_endpoint,
        "https://gproxy.example.com/oauth/revoke"
    );
    assert_eq!(metadata.response_types_supported, ["code"]);
    assert_eq!(
        metadata.grant_types_supported,
        ["authorization_code", "refresh_token", DEVICE_GRANT_TYPE]
    );
    assert_eq!(metadata.code_challenge_methods_supported, ["S256"]);
    assert_eq!(metadata.token_endpoint_auth_methods_supported, ["none"]);
    assert_eq!(
        metadata.revocation_endpoint_auth_methods_supported,
        ["none"]
    );

    // The same instance under a path mount answers with that mount, which is
    // the whole reason the origin is passed in rather than computed here.
    let mounted = issuer.metadata(
        &IssuerOrigin::new("https://gproxy.example.com")
            .unwrap()
            .mounted_at("/acme/v1")
            .unwrap(),
    );
    assert_eq!(mounted.issuer, "https://gproxy.example.com/acme/v1");
    assert_eq!(
        mounted.authorization_endpoint,
        "https://gproxy.example.com/acme/v1/oauth/authorize"
    );

    // And the document is JSON with the RFC's own field names.
    let text = serde_json::to_string(&metadata).unwrap();
    assert!(
        text.contains("\"code_challenge_methods_supported\""),
        "{text}"
    );
    assert!(text.contains("\"device_authorization_endpoint\""), "{text}");
}
