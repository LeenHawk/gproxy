#![cfg(not(target_arch = "wasm32"))]
//! Admission against a real database: one `load_all_data()` becomes one
//! `AppData`, a `Caller` goes in, and an `Admitted` comes out with the
//! provider set, the credential set, the budget chain, the scope, the session
//! and the rate-limit charges the engine will run with.

use gproxy_app::{
    Admission, AdmissionRequest, AppConfig, AppData, Authenticator, Caller, CallerKind,
    admission::{GATEWAY_SESSION_HEADER, RateLease},
    snapshot::encode_key_hash,
};
use gproxy_cache::{Cache, Limits, MemoryCache, MemoryOptions};
use gproxy_core::SessionSource;
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        identity::{
            api_key::{self, ApiKeyKind},
            organization, permission, team, user,
        },
        limits::rate_limit,
        oauth,
        upstream::{credential, provider},
    },
};
use http::HeaderMap;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};

/// A fixed clock, so every rate-limit window boundary in this file is stated
/// rather than observed. It is 40 s into a 60 s window.
const NOW: i64 = 1_700_000_000_000;
const WINDOW_START: i64 = 1_699_999_980_000;

const ACCESS: &str = "oauth-access-token-plaintext";

fn digest_of(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn key(id: &str, user_id: &str) -> api_key::ActiveModel {
    api_key::ActiveModel {
        id: Set(id.into()),
        user_id: Set(user_id.into()),
        name: Set(id.into()),
        key_hash: Set(encode_key_hash(&digest_of(id))),
        prefix: Set("sk-".into()),
        ..Default::default()
    }
}

fn rule(
    id: &str,
    user_id: &str,
    provider_id: Option<&str>,
    pattern: &str,
    action: &str,
    priority: i32,
) -> permission::ActiveModel {
    permission::ActiveModel {
        id: Set(id.into()),
        user_id: Set(Some(user_id.into())),
        provider_id: Set(provider_id.map(Into::into)),
        model_pattern: Set(pattern.into()),
        action: Set(action.into()),
        priority: Set(priority),
        ..Default::default()
    }
}

fn limit(
    id: &str,
    user_id: Option<&str>,
    api_key_id: Option<&str>,
    metric: &str,
    value: i64,
    period_seconds: i64,
    pattern: &str,
) -> rate_limit::ActiveModel {
    rate_limit::ActiveModel {
        id: Set(id.into()),
        user_id: Set(user_id.map(Into::into)),
        api_key_id: Set(api_key_id.map(Into::into)),
        metric: Set(metric.into()),
        limit_value: Set(FixedDecimal::from_atoms(value * FixedDecimal::FACTOR)),
        period_seconds: Set(period_seconds),
        model_pattern: Set(Some(pattern.into())),
        enabled: Set(true),
    }
}

/// Two organizations, one team, five people, the key bindings that separate
/// them, three providers with a credential per owner kind, a permission truth
/// table over `alice`, and two rate limits over `dave`.
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
            config_revision: Set(4),
            ..Default::default()
        })
        .await
        .unwrap();

    let person = |id: &str, role: &str| user::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        role: Set(role.into()),
        created_at_ms: Set(0),
        ..Default::default()
    };
    store
        .users()
        .create_many(vec![
            person("root", "admin"),
            person("alice", "user"),
            person("bob", "user"),
            person("dave", "user"),
        ])
        .await
        .unwrap();
    let org = |id: &str| organization::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        created_at_ms: Set(0),
        ..Default::default()
    };
    store
        .organizations()
        .create_many(vec![org("acme"), org("globex")])
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

    let mut team_key = key("k-alice-team", "alice");
    team_key.team_id = Set(Some("core".into()));
    let mut globex_key = key("k-alice-globex", "alice");
    globex_key.organization_id = Set(Some("globex".into()));
    let mut acme_key = key("k-alice-acme", "alice");
    acme_key.organization_id = Set(Some("acme".into()));
    let mut grant_key = key("k-grant", "alice");
    grant_key.kind = Set(ApiKeyKind::OAuth);
    store
        .api_keys()
        .create_many(vec![
            key("k-alice", "alice"),
            team_key,
            globex_key,
            acme_key,
            key("k-root", "root"),
            key("k-bob", "bob"),
            key("k-dave", "dave"),
            grant_key,
        ])
        .await
        .unwrap();

    let upstream = |id: &str| provider::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        channel: Set("custom".into()),
        config: Set(json!({})),
        created_at_ms: Set(0),
        ..Default::default()
    };
    store
        .providers()
        .create_many(vec![
            upstream("openai"),
            upstream("anthropic"),
            upstream("google"),
        ])
        .await
        .unwrap();
    let secret = |id: &str, user: Option<&str>, team: Option<&str>, org: Option<&str>| {
        credential::ActiveModel {
            id: Set(id.into()),
            provider_id: Set("openai".into()),
            user_id: Set(user.map(Into::into)),
            team_id: Set(team.map(Into::into)),
            organization_id: Set(org.map(Into::into)),
            auth_kind: Set("api_key".into()),
            secret: Set(Vec::new()),
            metadata: Set(json!({})),
            ..Default::default()
        }
    };
    // The truth table. `alice` may use everything except Google, except
    // `gpt-*`, except `gpt-4o` on OpenAI.
    store
        .permissions()
        .create_many(vec![
            rule("p-allow-all", "alice", None, "*", "allow", 0),
            rule("p-deny-google", "alice", Some("google"), "*", "deny", 5),
            rule("p-deny-gpt", "alice", None, "gpt-*", "deny", 10),
            rule("p-allow-4o", "alice", Some("openai"), "gpt-4o", "allow", 20),
            rule("p-dave", "dave", None, "*", "allow", 0),
        ])
        .await
        .unwrap();

    store
        .credentials()
        .create_many(vec![
            secret("c-shared", None, None, None),
            secret("c-alice", Some("alice"), None, None),
            secret("c-bob", Some("bob"), None, None),
            secret("c-core", None, Some("core"), None),
            secret("c-acme", None, None, Some("acme")),
            secret("c-globex", None, None, Some("globex")),
        ])
        .await
        .unwrap();

    // Scoped by model pattern so one test can exercise one of them.
    store
        .rate_limits()
        .create_many(vec![
            limit(
                "rl-counted",
                Some("dave"),
                None,
                "requests",
                2,
                60,
                "counted-*",
            ),
            limit(
                "rl-parallel",
                None,
                Some("k-dave"),
                "concurrency",
                1,
                3600,
                "parallel-*",
            ),
        ])
        .await
        .unwrap();

    store
        .oauth_clients()
        .create_many(vec![oauth::client::ActiveModel {
            id: Set("third-party".into()),
            name: Set("Third Party".into()),
            redirect_uris: Set(json!(["http://localhost:9999/callback"])),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_grants()
        .create_many(vec![oauth::grant::ActiveModel {
            id: Set("g-alice".into()),
            user_id: Set("alice".into()),
            api_key_id: Set("k-grant".into()),
            client_id: Set("third-party".into()),
            scopes: Set(json!([
                "gproxy:generate_content",
                "gproxy:create_embedding"
            ])),
            subject: Set("alice".into()),
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
            grant_id: Set("g-alice".into()),
            kind: Set(oauth::token::TokenKind::Access),
            created_at_ms: Set(0),
            expires_at_ms: Set(NOW + 3_600_000),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
}

/// The snapshot, and the instance's live provider and credential sets, all
/// from the same read.
async fn world(store: &Store<DatabaseConnection>) -> (AppData, BTreeSet<String>, BTreeSet<String>) {
    let all = store.load_all_data().await.unwrap();
    let revision = all.control.settings.as_ref().unwrap().config_revision;
    let providers = all
        .control
        .providers
        .iter()
        .map(|row| row.id.clone())
        .collect();
    let credentials = all
        .control
        .credentials
        .iter()
        .map(|row| row.id.clone())
        .collect();
    let data =
        AppData::assemble_at(revision, &all.identity, &all.control.credentials, NOW).unwrap();
    (data, providers, credentials)
}

/// The caller behind one seeded key, through the real authenticator: the
/// binding admission enforces has to be the one authentication produced.
async fn caller_for(store: &Store<DatabaseConnection>, snapshot: &AppData, key_id: &str) -> Caller {
    let config = AppConfig::default();
    Authenticator::new(store, snapshot, &config)
        .authenticate_token_at(key_id, NOW)
        .await
        .unwrap()
}

fn cache() -> Arc<dyn Cache> {
    Arc::new(MemoryCache::new(MemoryOptions::default()).unwrap())
}

/// A cache that refuses every key it is given, which is what an unreachable
/// backend looks like from here.
fn broken_cache() -> Arc<dyn Cache> {
    Arc::new(
        MemoryCache::new(MemoryOptions {
            limits: Limits {
                max_key_bytes: 1,
                ..Limits::default()
            },
            ..MemoryOptions::default()
        })
        .unwrap(),
    )
}

fn generate() -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect: Dialect::OpenAi,
    }
}

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

fn sorted(set: &BTreeSet<String>) -> Vec<&str> {
    set.iter().map(String::as_str).collect()
}

/// The drop path spawns its cache release; a test observing the effect has to
/// let the runtime run it.
async fn settle() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

// --- the permission truth table -----------------------------------------

/// `admit` for one caller and model, with no rate-limit interference.
async fn admit_model(
    store: &Store<DatabaseConnection>,
    config: &AppConfig,
    key_id: &str,
    model: Option<&str>,
    operation: OperationKey,
) -> Result<gproxy_app::Admitted, gproxy_app::AppError> {
    let (snapshot, providers, credentials) = world(store).await;
    let caller = caller_for(store, &snapshot, key_id).await;
    let cache = cache();
    let admission = Admission::new(store, &snapshot, &cache, config);
    let map = headers(&[]);
    admission
        .admit(
            AdmissionRequest::new(&caller, operation, &map, "req-1", &providers, &credentials)
                .model(model)
                .at(NOW),
        )
        .await
}

#[tokio::test]
async fn a_deny_at_a_higher_priority_beats_an_allow_below_it() {
    let store = seeded().await;
    let config = AppConfig::default();
    // `p-allow-all` (0) grants everything; `p-deny-gpt` (10) takes `gpt-*`
    // back on every provider, so nothing is left.
    let error = admit_model(&store, &config, "k-alice", Some("gpt-4-turbo"), generate())
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 403);
    assert!(error.to_string().contains("no provider is permitted"));
}

#[tokio::test]
async fn an_allow_at_a_higher_priority_beats_a_broad_deny_below_it() {
    let store = seeded().await;
    let config = AppConfig::default();
    // `p-allow-4o` (20, openai) is reached before `p-deny-gpt` (10), and only
    // on OpenAI: the other two providers are still refused the model.
    let admitted = admit_model(&store, &config, "k-alice", Some("gpt-4o"), generate())
        .await
        .unwrap();
    assert_eq!(sorted(&admitted.providers), ["openai"]);
}

#[tokio::test]
async fn a_provider_scoped_deny_only_removes_that_provider() {
    let store = seeded().await;
    let config = AppConfig::default();
    let admitted = admit_model(&store, &config, "k-alice", Some("claude-3"), generate())
        .await
        .unwrap();
    assert_eq!(sorted(&admitted.providers), ["anthropic", "openai"]);
}

#[tokio::test]
async fn a_caller_no_rule_reaches_is_refused() {
    let store = seeded().await;
    let config = AppConfig::default();
    // `bob` holds a valid key and has no permission row at all.
    let error = admit_model(&store, &config, "k-bob", Some("claude-3"), generate())
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 403);
}

#[tokio::test]
async fn an_instance_admin_bypasses_the_rules_and_sees_everything() {
    let store = seeded().await;
    let config = AppConfig::default();
    // `root` has no permission row either, and a model every rule denies.
    let admitted = admit_model(&store, &config, "k-root", Some("gpt-4-turbo"), generate())
        .await
        .unwrap();
    assert_eq!(
        sorted(&admitted.providers),
        ["anthropic", "google", "openai"]
    );
    assert_eq!(
        sorted(&admitted.credentials),
        [
            "c-acme", "c-alice", "c-bob", "c-core", "c-globex", "c-shared"
        ]
    );
}

// --- the OAuth operation restriction ------------------------------------

async fn admit_grant(
    store: &Store<DatabaseConnection>,
    config: &AppConfig,
    operation: Operation,
) -> Result<gproxy_app::Admitted, gproxy_app::AppError> {
    let (snapshot, providers, credentials) = world(store).await;
    let caller = Authenticator::new(store, &snapshot, config)
        .authenticate_token_at(ACCESS, NOW)
        .await
        .unwrap();
    assert_eq!(caller.kind, CallerKind::OAuthGrant);
    let cache = cache();
    let admission = Admission::new(store, &snapshot, &cache, config);
    let map = headers(&[]);
    admission
        .admit(
            AdmissionRequest::new(
                &caller,
                OperationKey {
                    operation,
                    dialect: Dialect::OpenAi,
                },
                &map,
                "req-1",
                &providers,
                &credentials,
            )
            .model(Some("claude-3"))
            .at(NOW),
        )
        .await
}

#[tokio::test]
async fn an_unknown_oauth_client_gets_the_coding_agent_baseline_only() {
    let store = seeded().await;
    let config = AppConfig::default();
    assert!(
        admit_grant(&store, &config, Operation::GenerateContent)
            .await
            .is_ok()
    );
    let error = admit_grant(&store, &config, Operation::CreateEmbedding)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 403);
    assert!(error.to_string().contains("not available to OAuth clients"));
}

#[tokio::test]
async fn naming_the_client_as_a_cli_lifts_the_baseline() {
    let store = seeded().await;
    let mut config = AppConfig::default();
    config.oauth.cli_client_ids = vec!["third-party".into()];
    let admitted = admit_grant(&store, &config, Operation::CreateEmbedding)
        .await
        .unwrap();
    // And it is still the grant's own scope, charged to the grant's key.
    assert_eq!(admitted.scope, "grant:g-alice");
    let owners: Vec<String> = admitted.budgets.iter().map(ToString::to_string).collect();
    assert_eq!(owners, ["api_key:k-grant", "user:alice"]);
    assert_eq!(admitted.attribution.user_id.as_deref(), Some("alice"));
    assert_eq!(admitted.attribution.api_key_id.as_deref(), Some("k-grant"));
}

// --- credential visibility ----------------------------------------------

#[tokio::test]
async fn the_key_binding_decides_which_credentials_are_reachable() {
    let store = seeded().await;
    let config = AppConfig::default();
    let visible = async |key_id: &str| {
        let admitted = admit_model(&store, &config, key_id, Some("claude-3"), generate())
            .await
            .unwrap();
        admitted
            .credentials
            .iter()
            .map(String::to_owned)
            .collect::<Vec<String>>()
    };

    // A personal key: the shared ones and alice's own.
    assert_eq!(visible("k-alice").await, ["c-alice", "c-shared"]);
    // A team key also reaches the team's, and the parent organization's.
    assert_eq!(
        visible("k-alice-team").await,
        ["c-acme", "c-alice", "c-core", "c-shared"]
    );
    // Same person, a key bound to the other organization: acme is gone.
    assert_eq!(
        visible("k-alice-globex").await,
        ["c-alice", "c-globex", "c-shared"]
    );
    // An organization key does not reach into the team below it.
    assert_eq!(
        visible("k-alice-acme").await,
        ["c-acme", "c-alice", "c-shared"]
    );
}

#[tokio::test]
async fn nothing_visible_is_not_an_error_here() {
    let store = seeded().await;
    let config = AppConfig::default();
    let (snapshot, providers, _) = world(&store).await;
    let caller = caller_for(&store, &snapshot, "k-alice").await;
    let cache = cache();
    let admission = Admission::new(&store, &snapshot, &cache, &config);
    let map = headers(&[]);
    // A live set holding only somebody else's credential.
    let credentials = BTreeSet::from(["c-bob".to_string()]);
    let admitted = admission
        .admit(
            AdmissionRequest::new(&caller, generate(), &map, "req-1", &providers, &credentials)
                .model(Some("claude-3"))
                .at(NOW),
        )
        .await
        .unwrap();
    assert!(admitted.credentials.is_empty());
    assert!(!admitted.providers.is_empty());
}

// --- budget chain, scope, attribution ------------------------------------

#[tokio::test]
async fn the_budget_chain_follows_the_key_binding() {
    let store = seeded().await;
    let config = AppConfig::default();
    let chain = async |key_id: &str| {
        let admitted = admit_model(&store, &config, key_id, Some("claude-3"), generate())
            .await
            .unwrap();
        admitted
            .budgets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
    };
    assert_eq!(chain("k-alice").await, ["api_key:k-alice", "user:alice"]);
    assert_eq!(
        chain("k-alice-team").await,
        ["api_key:k-alice-team", "user:alice", "team:core"]
    );
    assert_eq!(
        chain("k-alice-acme").await,
        ["api_key:k-alice-acme", "user:alice", "org:acme"]
    );
}

#[tokio::test]
async fn the_scope_is_the_user_for_a_key_and_the_grant_for_a_grant() {
    let store = seeded().await;
    let config = AppConfig::default();
    let personal = admit_model(&store, &config, "k-alice", Some("claude-3"), generate())
        .await
        .unwrap();
    let team = admit_model(
        &store,
        &config,
        "k-alice-team",
        Some("claude-3"),
        generate(),
    )
    .await
    .unwrap();
    assert_eq!(personal.scope, "user:alice");
    // Two keys of one person are one caller's traffic.
    assert_eq!(team.scope, personal.scope);

    let grant = admit_grant(&store, &config, Operation::GenerateContent)
        .await
        .unwrap();
    assert_eq!(grant.scope, "grant:g-alice");
}

// --- the session ---------------------------------------------------------

#[tokio::test]
async fn a_request_with_no_session_carries_its_own_id() {
    let store = seeded().await;
    let config = AppConfig::default();
    let admitted = admit_model(&store, &config, "k-alice", Some("claude-3"), generate())
        .await
        .unwrap();
    let session = admitted.session.unwrap();
    assert_eq!(session.id, "req-1");
    assert_eq!(session.source, SessionSource::RequestFallback);
    assert!(!session.is_stable());
}

#[tokio::test]
async fn the_gateway_header_names_the_session() {
    let store = seeded().await;
    let config = AppConfig::default();
    let (snapshot, providers, credentials) = world(&store).await;
    let caller = caller_for(&store, &snapshot, "k-alice").await;
    let cache = cache();
    let admission = Admission::new(&store, &snapshot, &cache, &config);
    let map = headers(&[
        (GATEWAY_SESSION_HEADER, "conversation-7"),
        ("thread-id", "the-clients-own"),
    ]);
    let admitted = admission
        .admit(
            AdmissionRequest::new(&caller, generate(), &map, "req-1", &providers, &credentials)
                .model(Some("claude-3"))
                .at(NOW),
        )
        .await
        .unwrap();
    let session = admitted.session.unwrap();
    assert_eq!(session.id, "conversation-7");
    assert_eq!(session.source, SessionSource::Gateway);
}

// --- rate limits ---------------------------------------------------------

/// `admit` for `dave`, whose two limits are scoped by model pattern.
async fn admit_dave(
    store: &Store<DatabaseConnection>,
    cache: &Arc<dyn Cache>,
    model: &str,
    now_ms: i64,
) -> Result<gproxy_app::Admitted, gproxy_app::AppError> {
    let (snapshot, providers, credentials) = world(store).await;
    let caller = caller_for(store, &snapshot, "k-dave").await;
    let config = AppConfig::default();
    let admission = Admission::new(store, &snapshot, cache, &config);
    let map = headers(&[]);
    admission
        .admit(
            AdmissionRequest::new(&caller, generate(), &map, "req-1", &providers, &credentials)
                .model(Some(model))
                .at(now_ms),
        )
        .await
}

#[tokio::test]
async fn a_model_no_limit_covers_takes_no_charge() {
    let store = seeded().await;
    let cache = cache();
    let admitted = admit_dave(&store, &cache, "free-1", NOW).await.unwrap();
    assert!(admitted.rate_limit_leases.is_empty());
}

#[tokio::test]
async fn a_window_admits_up_to_its_ceiling_and_then_rejects() {
    let store = seeded().await;
    let cache = cache();
    for _ in 0..2 {
        let mut admitted = admit_dave(&store, &cache, "counted-1", NOW).await.unwrap();
        assert_eq!(admitted.rate_limit_leases.len(), 1);
        assert_eq!(admitted.rate_limit_leases[0].limit_id(), "rl-counted");
        assert_eq!(
            admitted.rate_limit_leases[0].key(),
            format!("rl/rl-counted/{WINDOW_START}")
        );
        // The request ran: the count stands.
        admitted.finish();
    }
    let error = admit_dave(&store, &cache, "counted-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 429);
    assert!(
        matches!(
            error,
            gproxy_app::AppError::RateLimited {
                retry_after_ms: Some(ms)
            } if ms == WINDOW_START + 60_000 - NOW
        ),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_next_window_starts_from_zero() {
    let store = seeded().await;
    let cache = cache();
    for _ in 0..2 {
        admit_dave(&store, &cache, "counted-1", NOW)
            .await
            .unwrap()
            .finish();
    }
    assert!(admit_dave(&store, &cache, "counted-1", NOW).await.is_err());
    let next = admit_dave(&store, &cache, "counted-1", NOW + 60_000)
        .await
        .unwrap();
    assert_eq!(
        next.rate_limit_leases[0].key(),
        format!("rl/rl-counted/{}", WINDOW_START + 60_000)
    );
}

#[tokio::test]
async fn a_refused_request_gives_its_charge_back() {
    let store = seeded().await;
    let cache = cache();
    // Dropped without `finish`: the request never ran, so the charge is
    // returned and the window still has its full allowance.
    for _ in 0..5 {
        drop(admit_dave(&store, &cache, "counted-1", NOW).await.unwrap());
        settle().await;
    }
    let counter = cache
        .counter(&format!("rl/rl-counted/{WINDOW_START}"))
        .await
        .unwrap();
    assert!(
        counter.is_none_or(|counter| counter.value == 0),
        "{counter:?}"
    );
}

#[tokio::test]
async fn a_concurrency_permit_is_held_for_the_request_and_returned_on_drop() {
    let store = seeded().await;
    let cache = cache();
    let mut held = admit_dave(&store, &cache, "parallel-1", NOW).await.unwrap();
    assert!(held.rate_limit_leases[0].is_permit());
    assert_eq!(held.rate_limit_leases[0].limit_id(), "rl-parallel");
    // The one slot is taken while the first request is in flight.
    let error = admit_dave(&store, &cache, "parallel-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 429);
    // Finishing does not keep the slot: it counts requests in flight.
    held.finish();
    drop(held);
    settle().await;
    assert!(admit_dave(&store, &cache, "parallel-1", NOW).await.is_ok());
}

#[tokio::test]
async fn an_awaited_release_needs_no_drop_path_at_all() {
    let store = seeded().await;
    let cache = cache();
    let mut held = admit_dave(&store, &cache, "parallel-1", NOW).await.unwrap();
    held.release().await;
    // No `settle`: the release has already happened.
    assert!(admit_dave(&store, &cache, "parallel-1", NOW).await.is_ok());
}

#[tokio::test]
async fn a_cache_that_cannot_answer_refuses_the_request() {
    let store = seeded().await;
    let cache = broken_cache();
    let error = admit_dave(&store, &cache, "counted-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 429);
    assert!(
        matches!(
            error,
            gproxy_app::AppError::RateLimited {
                retry_after_ms: None
            }
        ),
        "a cache failure carries no retry hint: {error:?}"
    );
    // And it is a refusal, not a pass: a model no limit covers is unaffected,
    // which is what shows the refusal came from the limit and not from a
    // blanket outage response.
    assert!(admit_dave(&store, &cache, "free-1", NOW).await.is_ok());
}

#[tokio::test]
async fn permissions_are_decided_before_any_counter_moves() {
    let store = seeded().await;
    let cache = cache();
    let (snapshot, providers, credentials) = world(&store).await;
    let caller = caller_for(&store, &snapshot, "k-dave").await;
    let config = AppConfig::default();
    let admission = Admission::new(&store, &snapshot, &cache, &config);
    let map = headers(&[]);
    // `dave` may use every provider, so narrow the live set to none: the
    // request is refused for permissions, before the `counted-*` limit.
    let empty = BTreeSet::new();
    let error = admission
        .admit(
            AdmissionRequest::new(&caller, generate(), &map, "req-1", &empty, &credentials)
                .model(Some("counted-1"))
                .at(NOW),
        )
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 403);
    assert!(
        cache
            .counter(&format!("rl/rl-counted/{WINDOW_START}"))
            .await
            .unwrap()
            .is_none(),
        "a request refused for permissions must not spend a window"
    );
    // The same request with providers available does spend one.
    let mut admitted = admission
        .admit(
            AdmissionRequest::new(&caller, generate(), &map, "req-1", &providers, &credentials)
                .model(Some("counted-1"))
                .at(NOW),
        )
        .await
        .unwrap();
    admitted.finish();
    assert_eq!(
        cache
            .counter(&format!("rl/rl-counted/{WINDOW_START}"))
            .await
            .unwrap()
            .unwrap()
            .value,
        1
    );
}

#[tokio::test]
async fn a_lease_reports_what_it_is_holding() {
    let store = seeded().await;
    let cache = cache();
    let admitted = admit_dave(&store, &cache, "parallel-1", NOW).await.unwrap();
    let lease: &RateLease = &admitted.rate_limit_leases[0];
    assert!(lease.is_permit());
    assert!(format!("{lease:?}").contains("rl-parallel"));
}
