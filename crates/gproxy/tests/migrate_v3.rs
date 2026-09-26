//! The v3 round trip: a real v3 export in, a working v4 instance out.
//!
//! This is not a unit test of a mapper. It builds the document a v3 instance
//! would have written — a provider with a credential sealed by v3's own
//! envelope cipher, two users, two API keys under the digest v3 stored, an
//! organization with a team and a member, a permission, a rate limit, a route
//! and an exposed name — imports it, and then asserts the three things that
//! decide whether the migration was worth running:
//!
//! 1. the migrated key **authenticates**, without the operator rotating it;
//! 2. its **permission is in force**, so it reaches the providers it was given
//!    and is refused the ones it was not;
//! 3. the provider and its credential are **visible and usable**, with the
//!    secret re-sealed under this instance's key rather than v3's.
//!
//! The same fixture is what `docs/` walks an operator through, and what the
//! `curl` session in the pull request was run against.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use gproxy::{
    config::{AdminOptions, Settings, TelemetryOptions},
    instance::{self, Instance},
};
use gproxy_app::{AppConfig, config::StoreBackendConfig};
use sea_orm::EntityTrait;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The master key the fixture's v3 instance sealed with.
const V3_MASTER_KEY: [u8; 32] = [0x5a; 32];

/// The key text an operator has been using against v3 all along. It is never
/// re-typed into v4: only its digest travels.
const ALICE_KEY: &str = "sk-alice-has-been-using-this-key-for-a-year";
const ADMIN_KEY: &str = "sk-the-v3-administrators-own-key";

/// What the v3 credential's secret actually was, so the test can prove the
/// bytes survived the two re-sealings.
fn upstream_secret() -> Value {
    json!({"api_key": "upstream-token-1234"})
}

// ----------------------------------------------------------- the fixture --

/// v3's envelope cipher, reimplemented from `v3:crates/gproxy-app/src/secrets.rs`.
/// Writing it out here rather than calling the migration's own opener is the
/// point: the fixture has to be bytes v3 would have produced, not this crate's
/// inverse of itself.
fn seal_like_v3(value: &Value, payload_aad: &[u8], wrapped_key_aad: &[u8]) -> Value {
    let master = Aes256Gcm::new(&Key::<Aes256Gcm>::from(V3_MASTER_KEY));
    let dek = [0x11_u8; 32];
    let payload_nonce = [0x01_u8; 12];
    let key_nonce = [0x02_u8; 12];
    let ciphertext = Aes256Gcm::new(&Key::<Aes256Gcm>::from(dek))
        .encrypt(
            &Nonce::from(payload_nonce),
            Payload {
                msg: &serde_json::to_vec(value).unwrap(),
                aad: payload_aad,
            },
        )
        .unwrap();
    let wrapped_key = master
        .encrypt(
            &Nonce::from(key_nonce),
            Payload {
                msg: &dek,
                aad: wrapped_key_aad,
            },
        )
        .unwrap();
    json!({
        "ciphertext": ciphertext,
        "wrapped_key": wrapped_key,
        "payload_nonce": payload_nonce.to_vec(),
        "key_nonce": key_nonce.to_vec(),
    })
}

fn sealed_credential(value: &Value) -> Value {
    seal_like_v3(
        value,
        b"gproxy:v3:credential-envelope:v1:payload",
        b"gproxy:v3:credential-envelope:v1:wrapped-dek",
    )
}

fn sealed_user_key(text: &str) -> Value {
    seal_like_v3(
        &Value::String(text.to_owned()),
        b"gproxy:v3:user-key-envelope:v1:payload",
        b"gproxy:v3:user-key-envelope:v1:wrapped-dek",
    )
}

/// v3's digest rule: SHA-256 of the key with its presentation prefix removed.
fn v3_digest(api_key: &str) -> Vec<u8> {
    let payload = api_key
        .strip_prefix("sk-")
        .or_else(|| api_key.strip_prefix("at-"))
        .unwrap_or(api_key);
    Sha256::digest(payload.as_bytes()).to_vec()
}

/// The document a v3 instance would have answered `POST /admin/api/export`
/// with, supplemented with the three lists v3's export handler forgets.
pub fn v3_export() -> Value {
    json!({
        "format_version": 1,
        "secrets": "included",
        "source_key": {"mode": "sealed", "fingerprint": "fixture"},
        "data": {
            "organizations": [{"id": 1, "name": "acme", "enabled": true}],
            "teams": [{"id": 1, "organization_id": 1, "name": "core", "enabled": true}],
            "users": [
                {"id": 1, "name": "root", "organization_id": 1, "team_id": null,
                 "enabled": true, "is_admin": true},
                {"id": 2, "name": "alice", "organization_id": 1, "team_id": 1,
                 "enabled": true, "is_admin": false}
            ],
            "user_keys": [
                {"config": {"id": 1, "user_id": 1, "prefix": "sk-", "label": "root key",
                            "expires_at": null, "enabled": true, "revealable": false},
                 "digest": v3_digest(ADMIN_KEY), "digest_version": 1, "secret": null},
                {"config": {"id": 2, "user_id": 2, "prefix": "sk-", "label": "alice laptop",
                            "expires_at": null, "enabled": true, "revealable": true},
                 "digest": v3_digest(ALICE_KEY), "digest_version": 1,
                 "secret": sealed_user_key(ALICE_KEY)}
            ],
            "providers": [{
                "id": 1, "name": "upstream", "label": "Upstream (prod)",
                "channel": "custom", "credential_strategy": "sticky",
                "proxy_url": null, "tls_fingerprint": null, "enabled": true,
                "settings": {"base_url": "http://127.0.0.1:1/v1",
                             "endpoints": {"claude_messages": "http://127.0.0.1:1/relay/messages"}}
            }],
            "credentials": [{
                "config": {"id": 1, "provider_id": 1, "label": "prod token",
                           "kind": "api_key", "version": 3, "enabled": true,
                           "weight": 100, "rpm_limit": 60, "tpm_limit": null,
                           "proxy_url": null},
                "secret": sealed_credential(&upstream_secret())
            }],
            "routes": [{"id": 1, "name": "claude", "strategy": "weighted",
                        "max_attempts": 2, "enabled": true}],
            "route_members": [{"id": 1, "route_id": 1, "provider_id": 1,
                               "upstream_model": "claude-sonnet-4", "tier": 0,
                               "weight": 100, "enabled": true}],
            "model_aliases": [{"id": 1, "name": "claude-sonnet-4", "route_id": 1,
                               "enabled": true}],
            "quotas": [{"id": 1, "subject_kind": "user_key", "subject_id": 2,
                        "quota_total": "25.00", "quota_daily": "1.00",
                        "enabled": true}],
            // The three v3's own export omits; see the module note in
            // `gproxy::v3::document`.
            "permissions": [{"id": 1, "subject_kind": "user_key", "subject_id": 2,
                             "provider_id": 1, "operation_group": null,
                             "model_pattern": "claude-*", "allowed": true}],
            "rate_limits": [{"id": 1, "subject_kind": "user", "subject_id": 2,
                             "requests": 120, "window_seconds": 60}],
            "provider_models": [{"id": 1, "provider_id": 1,
                                 "model_id": "claude-sonnet-4",
                                 "display_name": "Claude Sonnet 4",
                                 "context_window": 200000,
                                 "max_output_tokens": 64000,
                                 "metadata": {},
                                 "variants": ["claude-sonnet-4-thinking"],
                                 "enabled": true}]
        }
    })
}

// -------------------------------------------------------------- harness --

fn settings(directory: &std::path::Path, admin: AdminOptions) -> Settings {
    Settings {
        config: AppConfig {
            port: 0,
            data_dir: Some(directory.to_string_lossy().into_owned()),
            store: StoreBackendConfig::Sqlite {
                path: "gproxy.db".into(),
            },
            console: gproxy_app::config::ConsoleConfig {
                enabled: false,
                path: None,
            },
            ..AppConfig::default()
        },
        admin,
        telemetry: TelemetryOptions::default(),
        instance_id: None,
    }
}

async fn open(settings: &Settings) -> Instance {
    instance::open(settings, instance::OpenOptions::management())
        .await
        .expect("open the instance")
}

fn write_document(directory: &std::path::Path, document: &Value) -> std::path::PathBuf {
    let path = directory.join("v3-export.json");
    std::fs::write(&path, serde_json::to_vec_pretty(document).unwrap()).unwrap();
    path
}

/// The source key as an operator would type it: the hex of `GPROXY_MASTER_KEY`.
fn source_key() -> String {
    V3_MASTER_KEY
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ----------------------------------------------------------- round trip --

#[tokio::test]
async fn a_v3_deployment_becomes_a_working_v4_one() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(
        directory.path(),
        AdminOptions {
            user: "root".into(),
            password: Some("a-console-password".into()),
            api_key: None,
        },
    );
    let document = write_document(directory.path(), &v3_export());
    let instance = open(&settings).await;

    let report = gproxy::v3::import(
        &instance.app,
        &document,
        Some(&source_key()),
        &settings.admin,
        false,
    )
    .await
    .expect("import the v3 export");

    // ---- 1. the key authenticates, and it is the same text as on v3 -------
    let data = instance.app.data();
    let caller = instance
        .app
        .authenticator(&data)
        .authenticate_token(ALICE_KEY)
        .await
        .expect("alice's v3 key still authenticates");
    assert_eq!(caller.user_id, "v3-users-2");
    assert_eq!(caller.api_key_id.as_deref(), Some("v3-user_keys-2"));
    assert_eq!(caller.user_role, "user");
    // v3 let one key be spelled three ways, and that survives too.
    for spelling in [
        ALICE_KEY,
        &ALICE_KEY.replace("sk-", "at-"),
        ALICE_KEY.strip_prefix("sk-").unwrap(),
    ] {
        assert!(
            instance
                .app
                .authenticator(&data)
                .authenticate_token(spelling)
                .await
                .is_ok(),
            "`{spelling}` stopped working"
        );
    }
    // And the administrator's, with the role that lets it reach /admin/api.
    let admin = instance
        .app
        .authenticator(&data)
        .authenticate_token(ADMIN_KEY)
        .await
        .expect("the v3 administrator's key still authenticates");
    assert_eq!(admin.user_role, "admin");
    // A key that was never in the export does not.
    assert!(
        instance
            .app
            .authenticator(&data)
            .authenticate_token("sk-never-existed")
            .await
            .is_err()
    );

    // ---- 2. the permission is in force -----------------------------------
    use gproxy_app::admission::permission::allowed_providers;
    use gproxy_sdk::Operation;
    let providers: std::collections::BTreeSet<String> =
        ["v3-providers-1".to_owned()].into_iter().collect();

    let permitted = allowed_providers(
        &data,
        &caller,
        Some("claude-sonnet-4"),
        Operation::GenerateContent,
        &providers,
        &[],
    )
    .expect("the model the v3 permission allowed is allowed");
    assert_eq!(permitted, providers);

    let refused = allowed_providers(
        &data,
        &caller,
        Some("gpt-4o"),
        Operation::GenerateContent,
        &providers,
        &[],
    )
    .expect_err("a model outside the v3 pattern must still be refused");
    assert!(refused.to_string().contains("no provider is permitted"));

    // The administrator bypasses the filter, as v4 says it does.
    assert!(
        allowed_providers(
            &data,
            &admin,
            Some("gpt-4o"),
            Operation::GenerateContent,
            &providers,
            &[],
        )
        .is_ok()
    );

    // ---- 3. the provider, and a credential whose secret really opens -----
    let store = instance.app.gproxy().store();
    let provider = one(store.providers()).await;
    assert_eq!(provider.id, "v3-providers-1");
    // v3's name stays the invocation name; its label is the display name
    // (868c8eb87 separated the two).
    assert_eq!(provider.name, "upstream");
    assert_eq!(provider.display_name.as_deref(), Some("Upstream (prod)"));
    assert_eq!(provider.channel, "custom");
    assert_eq!(provider.base_url.as_deref(), Some("http://127.0.0.1:1/v1"));

    let credential = one(store.credentials()).await;
    assert_eq!(credential.id, "v3-credentials-1");
    assert_eq!(credential.provider_id, "v3-providers-1");
    assert_eq!(credential.auth_kind, "api_key");
    // The bytes are *this* instance's, not v3's: sealed by v4's codec under
    // v4's master key, and they open to exactly what v3 had stored.
    assert_ne!(credential.secret, Vec::<u8>::new());
    let opened = instance
        .app
        .gproxy()
        .core()
        .secret_codec()
        .open(&credential.id, &credential.secret)
        .expect("the migrated secret opens on this instance");
    assert_eq!(opened, upstream_secret());

    // ---- the rest of the deployment --------------------------------------
    assert_eq!(
        one(store.route_members()).await.upstream_model,
        "claude-sonnet-4"
    );
    assert_eq!(one(store.routes()).await.name, "claude-sonnet-4");
    assert_eq!(one(store.organizations()).await.name, "acme");
    assert_eq!(one(store.teams()).await.name, "core");
    assert_eq!(
        one(store.rate_limits()).await.limit_value.to_string(),
        "120"
    );
    assert_eq!(one(store.permissions()).await.model_pattern, "claude-*");
    assert_eq!(
        one(store.provider_models()).await.upstream_name,
        "claude-sonnet-4"
    );
    assert_eq!(
        one(store.provider_models()).await.metadata["variants"],
        serde_json::json!(["claude-sonnet-4-thinking"])
    );
    // v3's provider column and its `endpoints` setting both land where v4
    // reads them.
    assert_eq!(provider.config["credential_strategy"], "sticky");
    assert!(provider.config.get("endpoints").is_none());
    let endpoints = all(store.operation_endpoints()).await;
    assert_eq!(endpoints.len(), 2, "the call and its streaming form");
    assert!(endpoints.iter().all(|endpoint| endpoint.dialect == "claude"
        && endpoint.url == "http://127.0.0.1:1/relay/messages"));
    // v3's memberships: alice is in the organization and in the team, the
    // administrator only in the organization.
    assert_eq!(all(store.organization_members()).await.len(), 2);
    assert_eq!(all(store.team_members()).await.len(), 1);
    // One v3 quota row with two periods set, plus the credential's rpm limit.
    let mut quotas = all(store.quotas()).await;
    quotas.sort_by(|a, b| a.id.cmp(&b.id));
    let ids: Vec<_> = quotas.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "v3-credentials-1-rpm",
            "v3-quotas-1-daily",
            "v3-quotas-1-total"
        ]
    );

    // The administrator got the password the flag supplied; v3's export has
    // none to carry.
    let root = store
        .users()
        .get_many(&["v3-users-1".to_owned()])
        .await
        .unwrap()
        .into_iter()
        .next()
        .flatten()
        .unwrap();
    assert!(root.password_hash.is_some());
    assert_eq!(root.role, "admin");

    // The report is honest about the one thing this fixture could not carry.
    assert!(report.rows() > 0);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("password")),
        "{:?}",
        report.warnings
    );

    instance.app.gproxy().shutdown();
}

/// The recovery path: the same document a second time completes rather than
/// duplicates, because every id is derived from the v3 row.
#[tokio::test]
async fn importing_the_same_document_twice_is_the_same_deployment() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let document = write_document(directory.path(), &v3_export());
    let instance = open(&settings).await;

    for attempt in 1..=2 {
        gproxy::v3::import(
            &instance.app,
            &document,
            Some(&source_key()),
            &settings.admin,
            false,
        )
        .await
        .unwrap_or_else(|error| panic!("import {attempt} failed: {error}"));
    }

    let store = instance.app.gproxy().store();
    assert_eq!(all(store.providers()).await.len(), 1);
    assert_eq!(all(store.credentials()).await.len(), 1);
    assert_eq!(all(store.users()).await.len(), 2);
    assert_eq!(all(store.api_keys()).await.len(), 2);
    assert_eq!(all(store.permissions()).await.len(), 1);
    assert_eq!(all(store.organization_members()).await.len(), 2);

    // And the key still works after the second pass, which is the way a
    // re-import could plausibly break it.
    let data = instance.app.data();
    assert!(
        instance
            .app
            .authenticator(&data)
            .authenticate_token(ALICE_KEY)
            .await
            .is_ok()
    );

    instance.app.gproxy().shutdown();
}

// --------------------------------------------------------- the refusals --

#[tokio::test]
async fn the_wrong_master_key_fails_the_whole_import_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let document = write_document(directory.path(), &v3_export());
    let instance = open(&settings).await;

    let wrong: String = [0xab_u8; 32]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let error = gproxy::v3::import(
        &instance.app,
        &document,
        Some(&wrong),
        &settings.admin,
        false,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("--source-master-key"), "{error}");
    assert!(error.contains("refused as a whole"), "{error}");

    // Nothing at all: the failure happens during translation, before the sdk
    // is handed anything and before a single identity row is written.
    let store = instance.app.gproxy().store();
    assert!(all(store.providers()).await.is_empty());
    assert!(all(store.credentials()).await.is_empty());
    assert!(all(store.users()).await.is_empty());

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn a_sealed_export_without_a_key_says_which_flag_is_missing() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let document = write_document(directory.path(), &v3_export());
    let instance = open(&settings).await;

    let error = gproxy::v3::import(&instance.app, &document, None, &settings.admin, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("GPROXY_MASTER_KEY"), "{error}");
    assert!(all(instance.app.gproxy().store().users()).await.is_empty());

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn a_key_under_a_digest_rule_this_build_does_not_know_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let mut export = v3_export();
    export["data"]["user_keys"][1]["digest_version"] = json!(2);
    let document = write_document(directory.path(), &export);
    let instance = open(&settings).await;

    let error = gproxy::v3::import(
        &instance.app,
        &document,
        Some(&source_key()),
        &settings.admin,
        false,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("digest version 2"), "{error}");
    assert!(error.contains("could never authenticate"), "{error}");

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn an_export_taken_without_secrets_is_refused_before_anything_is_written() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let mut export = v3_export();
    export["secrets"] = json!("omitted");
    export["data"]["credentials"][0]["secret"] = Value::Null;
    let document = write_document(directory.path(), &export);
    let instance = open(&settings).await;

    let error = gproxy::v3::import(
        &instance.app,
        &document,
        Some(&source_key()),
        &settings.admin,
        false,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("include_secrets"), "{error}");

    instance.app.gproxy().shutdown();
}

/// The "empty or nearly so" rule: a destination with configuration of its own
/// is refused rather than merged into, so a half-import is not reachable.
#[tokio::test]
async fn a_destination_that_already_has_configuration_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let document = write_document(directory.path(), &v3_export());
    let instance = open(&settings).await;

    // What `gproxy serve` would have created on a first start.
    gproxy::bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .unwrap();

    let error = gproxy::v3::import(
        &instance.app,
        &document,
        Some(&source_key()),
        &settings.admin,
        false,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("configuration of its own"), "{error}");
    assert!(error.contains("gproxy migrate"), "{error}");

    // Untouched: the bootstrap user is still the only one.
    assert_eq!(all(instance.app.gproxy().store().users()).await.len(), 1);

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn a_v4_export_handed_to_from_v3_says_which_flag_to_use() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let instance = open(&settings).await;

    let v4 = directory.path().join("v4.json");
    gproxy::transfer::export(&instance.app, &v4, false)
        .await
        .unwrap();
    let error = gproxy::v3::import(&instance.app, &v4, None, &settings.admin, false)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("import --in"), "{error}");

    instance.app.gproxy().shutdown();
}

/// A v3 instance that ran without `GPROXY_MASTER_KEY` wrote its secrets as
/// plain JSON. They still have to arrive sealed by *this* instance's key.
#[tokio::test]
async fn a_plaintext_v3_deployment_arrives_sealed() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let mut export = v3_export();
    export["source_key"] = json!({"mode": "plaintext"});
    export["data"]["credentials"][0]["secret"] = json!({
        "ciphertext": serde_json::to_vec(&upstream_secret()).unwrap(),
        "wrapped_key": [], "payload_nonce": [], "key_nonce": []
    });
    export["data"]["user_keys"][1]["secret"] = Value::Null;
    let document = write_document(directory.path(), &export);
    let instance = open(&settings).await;

    gproxy::v3::import(&instance.app, &document, None, &settings.admin, false)
        .await
        .expect("a plaintext v3 export needs no key");

    let credential = one(instance.app.gproxy().store().credentials()).await;
    let opened = instance
        .app
        .gproxy()
        .core()
        .secret_codec()
        .open(&credential.id, &credential.secret)
        .unwrap();
    assert_eq!(opened, upstream_secret());

    let data = instance.app.data();
    assert!(
        instance
            .app
            .authenticator(&data)
            .authenticate_token(ALICE_KEY)
            .await
            .is_ok(),
        "the key travels on its digest, not on its text"
    );

    instance.app.gproxy().shutdown();
}

// -------------------------------------------------------------- helpers --

async fn all<C, E>(repository: gproxy_store::Repository<'_, C, E>) -> Vec<E::Model>
where
    C: gproxy_seaorm::BatchConnectionTrait,
    E: sea_orm::EntityTrait,
    <E::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType: Clone + Eq + std::hash::Hash + Sync,
{
    repository
        .query(<E as sea_orm::EntityTrait>::find())
        .await
        .unwrap()
}

async fn one<C, E>(repository: gproxy_store::Repository<'_, C, E>) -> E::Model
where
    C: gproxy_seaorm::BatchConnectionTrait,
    E: sea_orm::EntityTrait,
    <E::PrimaryKey as sea_orm::PrimaryKeyTrait>::ValueType: Clone + Eq + std::hash::Hash + Sync,
{
    let mut rows = all(repository).await;
    assert_eq!(rows.len(), 1, "expected exactly one row");
    rows.remove(0)
}

/// A skipped channel must not leave references to its provider or credentials.
#[tokio::test]
async fn skipping_an_unmappable_provider_cascades_and_rounds_v3_prices() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let mut export = v3_export();
    let data = &mut export["data"];
    data["providers"].as_array_mut().unwrap().push(json!({
        "id": 99, "name": "legacy", "channel": "groq", "enabled": true
    }));
    let mut credential = data["credentials"][0].clone();
    credential["config"]["id"] = json!(99);
    credential["config"]["provider_id"] = json!(99);
    data["credentials"].as_array_mut().unwrap().push(credential);
    data["permissions"].as_array_mut().unwrap().push(json!({
        "id": 99, "subject_kind": "user", "subject_id": 2,
        "provider_id": 99, "allowed": true
    }));
    data["route_members"].as_array_mut().unwrap().push(json!({
        "id": 99, "route_id": 1, "provider_id": 99,
        "upstream_model": "legacy", "enabled": true
    }));
    data["quotas"].as_array_mut().unwrap().push(json!({
        "id": 99, "subject_kind": "credential", "subject_id": 99,
        "quota_total": "10", "enabled": true
    }));
    data["price_rules"] = json!([
        {"id": 1, "provider_id": 1, "model_pattern": "*", "enabled": true},
        {"id": 99, "provider_id": 99, "model_pattern": "*", "enabled": true}
    ]);
    data["price_rates"] = json!([
        {"id": 1, "rule_id": 1, "metric": "input_tokens", "unit_size": 1000000, "price": "0.0416666666666667"},
        {"id": 99, "rule_id": 99, "metric": "input_tokens", "unit_size": 1000000, "price": "1"}
    ]);
    let document = write_document(directory.path(), &export);
    let instance = open(&settings).await;
    let key = source_key();
    assert!(
        gproxy::v3::import(&instance.app, &document, Some(&key), &settings.admin, false)
            .await
            .is_err()
    );
    gproxy::v3::import(&instance.app, &document, Some(&key), &settings.admin, true)
        .await
        .unwrap();
    let store = instance.app.gproxy().store();
    assert_eq!(one(store.providers()).await.id, "v3-providers-1");
    assert_eq!(one(store.credentials()).await.id, "v3-credentials-1");
    assert_eq!(
        one(store.price_rates()).await.value.to_string(),
        "0.041666667"
    );
    use gproxy_store::entity::{identity::permission, limits::quota, routing::route_member};
    assert!(
        store
            .permissions()
            .query(permission::Entity::find())
            .await
            .unwrap()
            .iter()
            .all(|row| row.provider_id.as_deref() != Some("v3-providers-99"))
    );
    assert!(
        store
            .quotas()
            .query(quota::Entity::find())
            .await
            .unwrap()
            .iter()
            .all(|row| row.owner_id != "v3-credentials-99")
    );
    assert!(
        store
            .route_members()
            .query(route_member::Entity::find())
            .await
            .unwrap()
            .iter()
            .all(|row| row.provider_id != "v3-providers-99")
    );
    instance.app.gproxy().shutdown();
}

async fn v3_database(directory: &std::path::Path) -> String {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
    let db = Database::connect(format!(
        "sqlite://{}?mode=rwc",
        directory.join("gproxy.db").display()
    ))
    .await
    .unwrap();
    for sql in [
        "CREATE TABLE schema_migrations (version integer PRIMARY KEY)",
        "INSERT INTO schema_migrations VALUES (1)",
        "CREATE TABLE settings (key text PRIMARY KEY, value_json text)",
        "CREATE TABLE users (id integer PRIMARY KEY, name text, enabled integer, is_admin integer, password_hash text)",
        "CREATE TABLE user_keys (id integer PRIMARY KEY, user_id integer, prefix text, enabled integer, digest blob, digest_version integer)",
        "CREATE TABLE providers (id integer PRIMARY KEY, name text, channel text, settings_json text, enabled integer)",
        "INSERT INTO providers VALUES (1, 'upstream', 'custom', '{\"base_url\":\"http://127.0.0.1:1/v1\"}', 1)",
        "CREATE TABLE credentials (id integer PRIMARY KEY, provider_id integer, ciphertext blob, wrapped_key blob, payload_nonce blob, key_nonce blob, enabled integer, kind text)",
    ] {
        db.execute_unprepared(sql).await.unwrap();
    }
    // v3 used the same Argon2 PHC format; a password shorter than v4's new-user
    // policy still authenticates because this upgrade preserves the hash.
    let hash = gproxy_app::auth::password::hash("old").unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO users VALUES (1, 'root', 1, 1, ?)",
        [hash.clone().into()],
    ))
    .await
    .unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO user_keys VALUES (1, 1, 'sk-', 1, ?, 1)",
        [v3_digest(ADMIN_KEY).into()],
    ))
    .await
    .unwrap();
    let sealed = sealed_credential(&upstream_secret());
    let blobs: Vec<sea_orm::Value> = ["ciphertext", "wrapped_key", "payload_nonce", "key_nonce"]
        .iter()
        .map(|field| {
            serde_json::from_value::<Vec<u8>>(sealed[field].clone())
                .unwrap()
                .into()
        })
        .collect();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO credentials VALUES (1, 1, ?, ?, ?, ?, 1, 'api_key')",
        blobs,
    ))
    .await
    .unwrap();
    db.close().await.unwrap();
    hash
}

#[tokio::test]
async fn startup_upgrades_v3_in_place_and_keeps_passwords_keys_and_a_backup() {
    let directory = tempfile::tempdir().unwrap();
    let hash = v3_database(directory.path()).await;
    let mut settings = settings(directory.path(), AdminOptions::default());
    settings.config.master_key.key = gproxy_app::config::MasterKey::Hex(source_key());
    let first = open(&settings).await;
    let user = one(first.app.gproxy().store().users()).await;
    assert_eq!(user.password_hash.as_deref(), Some(hash.as_str()));
    assert!(gproxy_app::auth::password::verify(
        "old",
        user.password_hash.as_deref().unwrap()
    ));
    let data = first.app.data();
    assert!(
        first
            .app
            .authenticator(&data)
            .authenticate_token(ADMIN_KEY)
            .await
            .is_ok()
    );
    assert_eq!(
        one(first.app.gproxy().store().credentials()).await.id,
        "v3-credentials-1"
    );
    let backups: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bak"))
        .collect();
    assert_eq!(backups.len(), 1);
    let source = gproxy::v3::source::read(&backups[0]).await.unwrap();
    assert_eq!(
        source.data.users[0].password_hash.as_deref(),
        Some(hash.as_str())
    );
    first.app.gproxy().shutdown();
    drop(first);
    let second = open(&settings).await;
    assert_eq!(all(second.app.gproxy().store().users()).await.len(), 1);
    assert_eq!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "bak"))
            .count(),
        1
    );
    second.app.gproxy().shutdown();
}

#[tokio::test]
async fn a_failed_automatic_import_leaves_v3_in_place_for_the_next_start() {
    use sea_orm::Database;
    let directory = tempfile::tempdir().unwrap();
    v3_database(directory.path()).await;
    let mut settings = settings(directory.path(), AdminOptions::default());
    settings.config.master_key.key = gproxy_app::config::MasterKey::Hex("ab".repeat(32));
    assert!(
        instance::open(&settings, instance::OpenOptions::management())
            .await
            .is_err()
    );
    let source = Database::connect(format!(
        "sqlite://{}?mode=ro",
        directory.path().join("gproxy.db").display()
    ))
    .await
    .unwrap();
    assert!(matches!(
        gproxy::v3::detect::inspect(&source).await.unwrap(),
        gproxy::v3::detect::Verdict::Version3 { .. }
    ));
    source.close().await.unwrap();
    settings.config.master_key.key = gproxy_app::config::MasterKey::Hex(source_key());
    let instance = open(&settings).await;
    let data = instance.app.data();
    assert!(
        instance
            .app
            .authenticator(&data)
            .authenticate_token(ADMIN_KEY)
            .await
            .is_ok()
    );
    instance.app.gproxy().shutdown();
}

/// What only v3's database carries, and what this migration used to leave
/// behind: instance settings, the issuer's clients, the tokenizer's token and
/// vocabularies, a grouped permission, an operator's routing rule and a
/// provider alias.
#[tokio::test]
async fn a_v3_database_brings_its_settings_clients_tokenizer_and_routing() {
    use gproxy_store::entity::{identity::permission, oauth::client, upstream::operation_rule};
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};
    let directory = tempfile::tempdir().unwrap();
    v3_database(directory.path()).await;
    let db = Database::connect(format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("gproxy.db").display()
    ))
    .await
    .unwrap();
    for sql in [
        r#"INSERT INTO settings VALUES ('instance_name', '"prod"'), ('update_channel', '"staging"'), ('default_tokenizer_vocab', '"o200k"')"#,
        "CREATE TABLE oauth_clients (client_id text, name text, redirect_uris text, enabled integer, deleted_at integer)",
        r#"INSERT INTO oauth_clients VALUES ('my-cli', 'My CLI', '["http://localhost:1455/cb"]', 1, NULL), ('old-cli', 'Old', '[]', 1, 5)"#,
        "CREATE TABLE tokenizer_vocabs (name text, repository text, bytes blob, updated_at integer)",
        "INSERT INTO tokenizer_vocabs VALUES ('o200k', NULL, x'7b7d', 0)",
        "CREATE TABLE permissions (id integer, subject_kind text, subject_id integer, provider_id integer, operation_group text, model_pattern text, allowed integer)",
        "INSERT INTO permissions VALUES (1, 'user', 1, 1, 'models', NULL, 0)",
        "CREATE TABLE routing_rules (id integer, provider_id integer, operation text, kind text, implementation text, dest_operation text, dest_kind text, sort_order integer, enabled integer, origin text)",
        "INSERT INTO routing_rules VALUES (1, 1, 'generate_content', 'claude_messages', 'transform_to', 'generate_content', 'openai_chat', 0, 1, 'operator'), (2, 1, 'list_models', 'openai', 'passthrough', NULL, NULL, 0, 1, 'channel_default')",
        "CREATE TABLE aliases (id integer, alias text, target text, provider_id integer, priority integer, enabled integer)",
        "INSERT INTO aliases VALUES (1, 'fast', 'gpt-x', 1, 0, 1)",
        "CREATE TABLE tokenizer_auth (kind text, ciphertext blob, wrapped_key blob, payload_nonce blob, key_nonce blob, updated_at integer)",
    ] {
        db.execute_unprepared(sql)
            .await
            .unwrap_or_else(|error| panic!("{sql}: {error}"));
    }
    let sealed = sealed_credential(&json!("hf_secret"));
    let blobs: Vec<sea_orm::Value> = ["ciphertext", "wrapped_key", "payload_nonce", "key_nonce"]
        .iter()
        .map(|field| {
            serde_json::from_value::<Vec<u8>>(sealed[field].clone())
                .unwrap()
                .into()
        })
        .collect();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO tokenizer_auth VALUES ('hugging_face', ?, ?, ?, ?, 0)",
        blobs,
    ))
    .await
    .unwrap();
    db.close().await.unwrap();

    let mut settings = settings(directory.path(), AdminOptions::default());
    settings.config.master_key.key = gproxy_app::config::MasterKey::Hex(source_key());
    // Vocabularies are stored files; without storage they are reported instead.
    settings.config.file_storage = Some(gproxy_app::config::FileStorageConfig::Fs {
        root: "files".into(),
    });
    let instance = open(&settings).await;
    let gproxy = instance.app.gproxy();
    let store = gproxy.store();

    let setting = store.settings().get().await.unwrap().unwrap();
    assert_eq!(setting.instance_name, "prod");
    assert_eq!(setting.update_channel.as_deref(), Some("beta"));

    let clients = all(store.oauth_clients()).await;
    assert_eq!(
        clients
            .iter()
            .map(|c: &client::Model| c.id.as_str())
            .collect::<Vec<_>>(),
        ["my-cli"],
        "the retired client stays retired"
    );

    let tokenizer = gproxy.manage().tokenizer();
    assert_eq!(tokenizer.reveal_auth().await.unwrap(), "hf_secret");
    let vocabularies = tokenizer.vocabularies().await.unwrap();
    assert_eq!(vocabularies.len(), 1);
    assert!(vocabularies[0].is_default);

    let mut operations: Vec<Option<String>> = all(store.permissions())
        .await
        .into_iter()
        .map(|row: permission::Model| row.operation)
        .collect();
    operations.sort();
    assert_eq!(
        operations,
        [Some("get_model".to_owned()), Some("list_models".to_owned())],
        "a deny on the models group stays on the models group"
    );

    let rules: Vec<operation_rule::Model> = all(store.operation_rules()).await;
    assert_eq!(rules.len(), 1, "only the operator's rule");
    assert_eq!(rules[0].action, "routing");

    let model = one(store.provider_models()).await;
    assert_eq!(model.upstream_name, "gpt-x");
    assert_eq!(model.metadata["variants"], json!(["fast"]));
    gproxy.shutdown();
}
