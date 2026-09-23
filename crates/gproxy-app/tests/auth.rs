#![cfg(not(target_arch = "wasm32"))]
//! Authentication against a real database: the digest ladder, the refusals,
//! the OAuth access path and console sessions.

use gproxy_app::{
    AppConfig, AppData, Authenticator, CallerKind,
    auth::{self, password, verify_same_origin},
    snapshot::encode_key_hash,
};
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        identity::{
            api_key::{self, ApiKeyKind},
            organization, team, user, user_session,
        },
        oauth,
    },
};
use http::{HeaderMap, Method, header};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, EntityTrait, Set};
use serde_json::json;
use sha2::{Digest, Sha256};

/// A fixed clock. Every expiry in this file is stated relative to it, so a
/// boundary test cannot pass by accident of when it ran.
const NOW: i64 = 1_700_000_000_000;

/// The bare secret of the key stored the way v3 stored one: the digest of the
/// payload, with no presentation prefix.
const BARE: &str = "0123456789abcdef0123456789abcdef";
/// A key stored the way `generate_api_key` writes one: the digest of the whole
/// `sk-…` text.
const FULL: &str = "sk-fedcba9876543210fedcba9876543210";
/// The plaintext of the seeded OAuth access token.
const ACCESS: &str = "oauth-access-token-plaintext";

fn digest_of(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn key(id: &str, user_id: &str, key_hash: String) -> api_key::ActiveModel {
    api_key::ActiveModel {
        id: Set(id.into()),
        user_id: Set(user_id.into()),
        name: Set(id.into()),
        key_hash: Set(key_hash),
        prefix: Set("sk-".into()),
        ..Default::default()
    }
}

/// Three users, the six key rows the ladder has to tell apart, and one live
/// OAuth grant with an issued access token.
async fn seeded() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(7),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![
            user::ActiveModel {
                id: Set("alice".into()),
                name: Set("alice".into()),
                role: Set("admin".into()),
                created_at_ms: Set(0),
                ..Default::default()
            },
            user::ActiveModel {
                id: Set("bob".into()),
                name: Set("bob".into()),
                role: Set("user".into()),
                created_at_ms: Set(0),
                ..Default::default()
            },
            user::ActiveModel {
                id: Set("banned".into()),
                name: Set("banned".into()),
                role: Set("user".into()),
                enabled: Set(false),
                created_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .organizations()
        .create_many(vec![organization::ActiveModel {
            id: Set("acme".into()),
            name: Set("acme".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .teams()
        .create_many(vec![team::ActiveModel {
            id: Set("core".into()),
            organization_id: Set("acme".into()),
            name: Set("core".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let mut bare = key("k-bare", "alice", encode_key_hash(&digest_of(BARE)));
    bare.organization_id = Set(Some("acme".into()));
    let mut expired = key("k-expired", "bob", encode_key_hash(&digest_of("expired")));
    expired.expires_at_ms = Set(Some(NOW + 1_000));
    let mut disabled = key("k-off", "bob", encode_key_hash(&digest_of("disabled")));
    disabled.enabled = Set(false);
    let mut oauth_key = key(
        "k-oauth",
        "bob",
        encode_key_hash(&digest_of("sk-oauth-key")),
    );
    oauth_key.kind = Set(ApiKeyKind::OAuth);
    oauth_key.team_id = Set(Some("core".into()));
    store
        .api_keys()
        .create_many(vec![
            bare,
            key("k-full", "bob", encode_key_hash(&digest_of(FULL))),
            expired,
            disabled,
            key("k-banned", "banned", encode_key_hash(&digest_of("banned"))),
            oauth_key,
        ])
        .await
        .unwrap();
    store
        .oauth_clients()
        .create_many(vec![oauth::client::ActiveModel {
            id: Set("codex".into()),
            name: Set("Codex CLI".into()),
            redirect_uris: Set(json!(["http://localhost:1455/callback"])),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_grants()
        .create_many(vec![oauth::grant::ActiveModel {
            id: Set("g-bob".into()),
            user_id: Set("bob".into()),
            api_key_id: Set("k-oauth".into()),
            client_id: Set("codex".into()),
            scopes: Set(json!(["openid", "profile"])),
            subject: Set("bob".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_tokens()
        .create_many(vec![oauth::token::ActiveModel {
            id: Set("t-access".into()),
            token_hash: Set(digest_of(ACCESS).to_vec()),
            grant_id: Set("g-bob".into()),
            kind: Set(oauth::token::TokenKind::Access),
            created_at_ms: Set(0),
            expires_at_ms: Set(NOW + 3_600_000),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
}

/// The snapshot as it stands at [`NOW`], which is when every key row above is
/// still within its lifetime.
async fn data(store: &Store<DatabaseConnection>) -> AppData {
    let all = store.load_all_data().await.unwrap();
    let revision = all.control.settings.as_ref().unwrap().config_revision;
    AppData::assemble_at(revision, &all.identity, &all.control.credentials, NOW).unwrap()
}

fn config() -> AppConfig {
    AppConfig {
        session_ttl_secs: 60,
        ..AppConfig::default()
    }
}

// --- the digest ladder ---------------------------------------------------

#[tokio::test]
async fn one_key_authenticates_under_every_spelling_of_itself() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    for spelling in [BARE, &format!("sk-{BARE}"), &format!("at-{BARE}")] {
        let caller = auth.authenticate_token_at(spelling, NOW).await.unwrap();
        assert_eq!(caller.api_key_id.as_deref(), Some("k-bare"), "{spelling}");
        assert_eq!(caller.user_id, "alice");
        assert_eq!(caller.user_role, "admin");
        assert!(caller.is_instance_admin());
        assert_eq!(caller.organization_id.as_deref(), Some("acme"));
        assert_eq!(caller.kind, CallerKind::ApiKey);
        assert!(caller.grant.is_none());
    }
}

#[tokio::test]
async fn a_key_stored_under_its_whole_text_authenticates_too() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let caller = auth.authenticate_token_at(FULL, NOW).await.unwrap();
    assert_eq!(caller.api_key_id.as_deref(), Some("k-full"));
    assert_eq!(caller.user_role, "user");
    // The payload alone is a different digest, and no row holds it.
    let error = auth
        .authenticate_token_at(FULL.trim_start_matches("sk-"), NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
}

#[tokio::test]
async fn the_ladder_is_reachable_through_the_headers_every_dialect_uses() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    for (name, value) in [
        (header::AUTHORIZATION.as_str(), format!("Bearer sk-{BARE}")),
        ("x-api-key", format!("sk-{BARE}")),
        ("x-goog-api-key", BARE.to_string()),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
        let caller = auth.authenticate_request(&headers).await.unwrap();
        assert_eq!(caller.api_key_id.as_deref(), Some("k-bare"), "{name}");
    }
    let error = auth
        .authenticate_request(&HeaderMap::new())
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
}

// --- refusals ------------------------------------------------------------

#[tokio::test]
async fn every_unusable_credential_is_unauthorized_and_says_nothing_else() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    for token in [
        "disabled",      // the key row is disabled
        "banned",        // the owning user is disabled
        "sk-oauth-key",  // an OAuth grant's internal key
        "never-existed", // no row at all
        "",              // no credential
        "   ",
    ] {
        let error = auth.authenticate_token_at(token, NOW).await.unwrap_err();
        assert_eq!(error.status_code(), 401, "{token}");
        assert_eq!(error.code(), "unauthorized", "{token}");
        // The reason stays in the operator's log, never in the response.
        assert_eq!(error.to_string(), "unauthorized", "{token}");
    }
}

#[tokio::test]
async fn a_key_that_expires_while_the_snapshot_is_live_stops_working() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    // Indexed at NOW, because it expires at NOW + 1_000.
    assert!(auth.authenticate_token_at("expired", NOW).await.is_ok());
    assert!(
        auth.authenticate_token_at("expired", NOW + 999)
            .await
            .is_ok()
    );
    // The boundary is exclusive, and the snapshot has not been rebuilt.
    let error = auth
        .authenticate_token_at("expired", NOW + 1_000)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
}

#[tokio::test]
async fn an_oauth_key_never_falls_through_to_the_access_token_path() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    // It is indexed — the index does not filter by kind — and refused.
    assert!(
        snapshot
            .keys
            .lookup(&digest_of("sk-oauth-key"))
            .is_some_and(|found| found.kind == ApiKeyKind::OAuth)
    );
    let error = auth.authenticate_api_key("sk-oauth-key", NOW).unwrap_err();
    assert_eq!(error.status_code(), 401);
}

// --- OAuth access tokens -------------------------------------------------

#[tokio::test]
async fn an_access_token_resolves_to_its_grant_and_the_keys_binding() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let caller = auth.authenticate_token_at(ACCESS, NOW).await.unwrap();
    assert_eq!(caller.kind, CallerKind::OAuthGrant);
    assert_eq!(caller.user_id, "bob");
    assert_eq!(caller.user_role, "user");
    assert!(!caller.is_instance_admin());
    // The api key is the grant's internal one, and the binding is its row's.
    assert_eq!(caller.api_key_id.as_deref(), Some("k-oauth"));
    assert_eq!(caller.team_id.as_deref(), Some("core"));
    assert_eq!(caller.organization_id, None);

    let grant = caller.grant.as_ref().unwrap();
    assert_eq!(grant.grant_id, "g-bob");
    assert_eq!(grant.client_id, "codex");
    assert_eq!(grant.scopes, ["openid", "profile"]);
    assert_eq!(grant.access_digest, digest_of(ACCESS));
}

#[tokio::test]
async fn an_expired_revoked_or_unknown_access_token_is_unauthorized() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    // Past the token's own expiry.
    assert!(
        auth.authenticate_token_at(ACCESS, NOW + 3_600_001)
            .await
            .is_err()
    );
    // An access token carries no presentation prefix, so none is stripped:
    // `at-` in front of it is simply a different token.
    assert!(
        auth.authenticate_token_at(&format!("at-{ACCESS}"), NOW)
            .await
            .is_err()
    );

    store
        .oauth_grants()
        .revoke_many(&["g-bob".to_string()], NOW)
        .await
        .unwrap();
    let error = auth.authenticate_token_at(ACCESS, NOW).await.unwrap_err();
    assert_eq!(error.status_code(), 401);
}

#[tokio::test]
async fn a_disabled_client_takes_its_grants_with_it() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);
    assert!(auth.authenticate_token_at(ACCESS, NOW).await.is_ok());

    oauth::client::Entity::update(oauth::client::ActiveModel {
        id: Set("codex".into()),
        enabled: Set(false),
        ..Default::default()
    })
    .exec(store.connection())
    .await
    .unwrap();
    // The snapshot is untouched; liveness is decided in the statement that
    // reads the token, not from a cached client row.
    assert!(auth.authenticate_token_at(ACCESS, NOW).await.is_err());
}

// --- sessions ------------------------------------------------------------

#[tokio::test]
async fn a_session_is_created_used_and_revoked() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let issued = auth.create_session("alice", NOW).await.unwrap();
    assert_eq!(issued.expires_at_ms, NOW + 60_000);
    assert!(!issued.token.is_empty());
    assert_ne!(issued.token, issued.id);

    let caller = auth.authenticate_session(&issued.token, NOW).await.unwrap();
    assert_eq!(caller.kind, CallerKind::Session);
    assert_eq!(caller.user_id, "alice");
    assert_eq!(caller.user_role, "admin");
    // A session acts as the person: no key, and no binding of its own.
    assert_eq!(caller.api_key_id, None);
    assert_eq!(caller.organization_id, None);
    assert_eq!(caller.team_id, None);
    assert_eq!(caller.subject().api_key_id, None);

    assert!(auth.revoke_session(&issued.token).await.unwrap());
    let error = auth
        .authenticate_session(&issued.token, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
    // Revoking twice is not an error and is not a second logout.
    assert!(!auth.revoke_session(&issued.token).await.unwrap());
}

#[tokio::test]
async fn only_the_digest_of_a_session_token_is_stored() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let issued = auth.create_session("alice", NOW).await.unwrap();
    let rows = user_session::Entity::find()
        .all(store.connection())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, issued.id);
    assert_eq!(
        rows[0].token_hash,
        encode_key_hash(&digest_of(&issued.token))
    );
    assert!(!rows[0].token_hash.contains(&issued.token));
}

#[tokio::test]
async fn a_session_ends_exactly_at_its_expiry() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let issued = auth.create_session("bob", NOW).await.unwrap();
    assert!(
        auth.authenticate_session(&issued.token, NOW + 59_999)
            .await
            .is_ok()
    );
    let error = auth
        .authenticate_session(&issued.token, NOW + 60_000)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
}

#[tokio::test]
async fn a_session_of_a_disabled_user_stops_working() {
    let store = seeded().await;
    let config = config();

    let issued = {
        let snapshot = data(&store).await;
        let auth = Authenticator::new(&store, &snapshot, &config);
        let issued = auth.create_session("bob", NOW).await.unwrap();
        assert!(auth.authenticate_session(&issued.token, NOW).await.is_ok());
        issued
    };

    store
        .users()
        .update_many(vec![user::ActiveModel {
            id: Set("bob".into()),
            enabled: Set(false),
            ..Default::default()
        }])
        .await
        .unwrap();
    let snapshot = data(&store).await;
    let auth = Authenticator::new(&store, &snapshot, &config);
    let error = auth
        .authenticate_session(&issued.token, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
}

#[tokio::test]
async fn sessions_are_revoked_per_user_and_swept_by_expiry() {
    let store = seeded().await;
    let snapshot = data(&store).await;
    let config = config();
    let auth = Authenticator::new(&store, &snapshot, &config);

    let live = auth.create_session("alice", NOW).await.unwrap();
    // Created two minutes ago with a one minute TTL: already over.
    let stale = auth.create_session("alice", NOW - 120_000).await.unwrap();
    let other = auth.create_session("bob", NOW).await.unwrap();

    assert_eq!(auth.purge_expired_sessions(NOW).await.unwrap(), 1);
    assert!(auth.authenticate_session(&stale.token, NOW).await.is_err());
    assert!(auth.authenticate_session(&live.token, NOW).await.is_ok());

    assert_eq!(
        auth.revoke_all_sessions_for_user("alice").await.unwrap(),
        1,
        "only alice's remaining session"
    );
    assert!(auth.authenticate_session(&live.token, NOW).await.is_err());
    assert!(auth.authenticate_session(&other.token, NOW).await.is_ok());
}

// --- CSRF and passwords, through the public surface ----------------------

#[test]
fn csrf_guards_unsafe_methods_only() {
    let origin = |value: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "gproxy.local".parse().unwrap());
        if !value.is_empty() {
            headers.insert(header::ORIGIN, value.parse().unwrap());
        }
        headers
    };
    let allowed = ["https://console.example.com".to_string()];

    assert!(verify_same_origin(&Method::GET, &origin(""), &[]).is_ok());
    assert!(verify_same_origin(&Method::POST, &origin(""), &[]).is_err());
    assert!(verify_same_origin(&Method::POST, &origin("https://gproxy.local"), &[]).is_ok());
    assert!(
        verify_same_origin(
            &Method::POST,
            &origin("https://console.example.com"),
            &allowed,
        )
        .is_ok()
    );
    assert!(verify_same_origin(&Method::POST, &origin("https://evil.example"), &allowed).is_err());
}

#[test]
fn passwords_round_trip_and_the_policy_is_enforced() {
    let phc = password::hash("correct horse battery staple").unwrap();
    assert!(password::verify("correct horse battery staple", &phc));
    assert!(!password::verify("wrong", &phc));
    assert!(!password::verify("correct horse battery staple", "garbage"));
    assert!(password::validate("correct horse battery staple").is_ok());
    assert_eq!(password::validate("short").unwrap_err().status_code(), 400);
    assert_eq!(password::validate("   ").unwrap_err().status_code(), 400);
}

#[test]
fn a_minted_key_is_found_by_the_ladder_that_will_check_it() {
    let (token, prefix, key_hash) = auth::generate_api_key(auth::API_KEY_PREFIX).unwrap();
    assert!(token.starts_with("sk-"));
    assert_eq!(prefix.len(), 8);
    assert!(token.contains(&prefix));
    assert_eq!(key_hash, encode_key_hash(&auth::digests(&token)[0]));
}
