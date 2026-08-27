use base64::Engine as _;
use bytes::Bytes;
use gproxy_core::CacheBackend;
use http::{HeaderMap, HeaderValue, Method};
use rust_decimal::Decimal;
use serde_json::json;

use crate::{App, AppHandle, Config, ControlMutation, MutationResult};

pub(super) struct Fixture {
    pub app: AppHandle,
    pub provider: i64,
    pub credential: i64,
    pub route: i64,
    pub quota: i64,
    pub client_key: String,
    pub _directory: tempfile::TempDir,
}

pub(super) async fn fixture() -> Fixture {
    let directory = tempfile::tempdir().expect("app tempdir");
    let mut master_key = [0_u8; 32];
    getrandom::fill(&mut master_key).expect("master key randomness");
    let upstream_key = random_key();
    let client_key = random_key();
    let config = Config::sqlite(
        "127.0.0.1:0".parse().unwrap(),
        directory.path().to_path_buf(),
        crate::MasterKeyConfig::new(Some(master_key)),
    );
    let app = App::start(config).await.expect("start app");
    let provider = id(app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "provider".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            },
        ))
        .await
        .expect("provider"));
    let credential = id(app
        .mutate(ControlMutation::Credential {
            provider_id: provider,
            label: None,
            secret: json!({"api_key": upstream_key}),
            enabled: true,
        })
        .await
        .expect("credential"));
    let route = id(app
        .mutate(ControlMutation::Route(gproxy_store::records::RouteInput {
            name: "route".into(),
            max_attempts: 1,
            enabled: true,
        }))
        .await
        .expect("route"));
    app.mutate(ControlMutation::RouteMember(
        gproxy_store::records::RouteMemberInput {
            route_id: route,
            provider_id: provider,
            credential_id: Some(credential),
            upstream_model: "upstream-model".into(),
            tier: 0,
            weight: 100,
            enabled: true,
        },
    ))
    .await
    .expect("route member");
    app.mutate(ControlMutation::ExposedModel(
        gproxy_store::records::ExposedModelInput {
            name: "public-model".into(),
            route_id: route,
            enabled: true,
        },
    ))
    .await
    .expect("model");
    let user = id(app
        .mutate(ControlMutation::User(gproxy_store::records::UserInput {
            name: "user".into(),
            organization_id: None,
            team_id: None,
            enabled: true,
        }))
        .await
        .expect("user"));
    let user_key = id(app
        .mutate(ControlMutation::UserKey {
            user_id: user,
            api_key: client_key.clone(),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await
        .expect("user key"));
    app.mutate(ControlMutation::Permission(
        gproxy_store::records::PermissionInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            provider_id: None,
            operation_group: Some("generate_content".into()),
            allowed: true,
        },
    ))
    .await
    .expect("permission");
    let quota = id(app
        .mutate(ControlMutation::Quota(gproxy_store::records::QuotaInput {
            subject_kind: "user_key".into(),
            subject_id: user_key,
            quota_total: Decimal::ONE,
            quota_daily: None,
            quota_weekly: None,
            quota_monthly: None,
            quota_5h: Some(Decimal::new(3, 1)),
            quota_7d: None,
            enabled: true,
        }))
        .await
        .expect("quota"));
    let price_rule = id(app
        .mutate(ControlMutation::PriceRule(
            gproxy_store::records::PriceRuleInput {
                provider_id: Some(provider),
                model_pattern: "upstream-model".into(),
                tiers: None,
                priority: 0,
                enabled: true,
            },
        ))
        .await
        .expect("price rule"));
    app.mutate(ControlMutation::PriceRate(
        gproxy_store::records::PriceRateInput {
            rule_id: price_rule,
            metric: "input_tokens".into(),
            unit_size: 1,
            price: Decimal::new(1, 2),
            conditions: None,
            priority: 0,
        },
    ))
    .await
    .expect("input price");
    Fixture {
        app,
        provider,
        credential,
        route,
        quota,
        client_key,
        _directory: directory,
    }
}

pub(super) fn random_key() -> String {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).expect("API key randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn id(result: MutationResult) -> i64 {
    let MutationResult::Id(id) = result else {
        panic!("mutation returned no id")
    };
    id
}

pub(super) fn request(id: &str, input: &str, api_key: &str) -> gproxy_core::RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}")).expect("authorization header"),
    );
    gproxy_core::RequestCtx {
        request_id: format!("request-{id}"),
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(json!({"model": "public-model", "input": input}).to_string()),
        upgrade: false,
        mode: gproxy_core::RoutingMode::Aggregated,
    }
}

pub(super) async fn counter(host: &crate::host::AppHost, window_id: i64) -> i64 {
    let value = host
        .services
        .cache
        .get(&format!("gproxy:quota-pending:{window_id}"))
        .await
        .expect("cache")
        .expect("quota counter");
    i64::from_be_bytes(value.try_into().expect("counter bytes"))
}

#[tokio::test]
async fn v2_digest_crossing_authenticates_the_unreissued_key() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    let mut source_key = [0_u8; 32];
    let mut target_key = [0_u8; 32];
    getrandom::fill(&mut source_key).unwrap();
    getrandom::fill(&mut target_key).unwrap();
    let stored_key = serde_json::to_string(&v2_seal(&json!(api_key), source_key)).unwrap();
    let stored_credential = v2_seal(&json!({"api_key": random_key()}), source_key);
    v2_database(
        source.path(),
        &api_key,
        &stored_key,
        &stored_credential,
        true,
    );
    let config = Config::sqlite(
        "127.0.0.1:0".parse().unwrap(),
        target.path().into(),
        crate::MasterKeyConfig::new(Some(target_key)),
    );
    let report = crate::migrate_from_v2(
        &config,
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: Some(base64::engine::general_purpose::STANDARD.encode(source_key)),
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.applied, "{report}");

    let app = App::start(config).await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}")).unwrap(),
    );
    let identity = crate::host::authenticate_headers(&app.inner.host, &headers)
        .expect("migrated key authenticates");
    assert_eq!(identity.user_id, 1);
}

#[tokio::test]
async fn v2_unrecoverable_key_is_reported_before_target_open() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let broken = json!({"kek_id":"local-x","wrapped_dek":"bad","nonce":"bad","ciphertext":"bad"});
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &broken.to_string(),
        &json!({"api_key":random_key()}),
        false,
    );
    let report = crate::migrate_from_v2(
        &super::test_config(target.path(), crate::MasterKeyConfig::new(None)),
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.entity == "user_keys" && issue.row == "id=1")
    );
    assert!(!report.applied, "{report}");
    assert!(!target.path().join("gproxy.db").exists());
}

#[tokio::test]
async fn v2_dry_run_does_not_create_the_target_store() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":random_key()}),
        false,
    );
    let report = crate::migrate_from_v2(
        &super::test_config(target.path(), crate::MasterKeyConfig::new(None)),
        crate::V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: false,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.dry_run && report.issues.is_empty(), "{report}");
    assert!(!target.path().join("gproxy.db").exists());
}

#[tokio::test]
async fn v2_reimport_is_idempotent_and_preserves_usage_cost() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let api_key = format!("sk-{}", random_key());
    v2_database(
        source.path(),
        &api_key,
        &api_key,
        &json!({"api_key":random_key()}),
        true,
    );
    let options = || crate::V2ImportOptions {
        path: source.path().join("gproxy.db"),
        source_master_key: None,
        apply: true,
        merge: false,
    };
    let config = super::test_config(target.path(), crate::MasterKeyConfig::new(None));
    assert!(
        crate::migrate_from_v2(&config, options())
            .await
            .unwrap()
            .applied
    );
    let second = crate::migrate_from_v2(&config, options()).await.unwrap();
    assert!(second.already_imported && !second.applied, "{second}");
    let store = gproxy_store::Store::open(config.backend_config())
        .await
        .unwrap();
    assert_eq!(store.usage_count().await.unwrap(), 1);
    assert_eq!(
        store
            .usage_by_request("v2-request")
            .await
            .unwrap()
            .unwrap()
            .usage
            .cost,
        "12.34".parse::<Decimal>().unwrap()
    );
}

fn v2_database(
    directory: &std::path::Path,
    api_key: &str,
    stored_key: &str,
    credential: &serde_json::Value,
    with_usage: bool,
) {
    use tokio_rusqlite::rusqlite::{Connection, params};
    let connection = Connection::open(directory.join("gproxy.db")).unwrap();
    connection.execute_batch(V2_SCHEMA).unwrap();
    connection
        .execute("INSERT INTO orgs VALUES(1,'org',1)", [])
        .unwrap();
    connection
        .execute("INSERT INTO users VALUES(1,'user',1,NULL,1)", [])
        .unwrap();
    connection.execute("INSERT INTO providers VALUES(1,'provider','openai',NULL,'{}','round_robin',NULL,NULL,1)", []).unwrap();
    connection
        .execute(
            "INSERT INTO credentials VALUES(1,1,NULL,'api_key',?,100,NULL,NULL,NULL,NULL,1)",
            [credential.to_string()],
        )
        .unwrap();
    let digest = blake3::hash(api_key.strip_prefix("sk-").unwrap_or(api_key).as_bytes())
        .to_hex()
        .to_string();
    connection
        .execute(
            "INSERT INTO user_keys VALUES(1,1,?,?,2,NULL,1)",
            params![stored_key, digest],
        )
        .unwrap();
    if with_usage {
        connection.execute("INSERT INTO usages VALUES(1,'v2-request',1,NULL,1,1,1,NULL,1,1,NULL,'generate_content','openai_chat','model',2,3,0,1,0,0,0,'{}','12.34',4,'upstream','complete')", []).unwrap();
    }
}

fn v2_seal(value: &serde_json::Value, key: [u8; 32]) -> serde_json::Value {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
    let mut dek = [0_u8; 32];
    let mut key_nonce = [0_u8; 24];
    let mut payload_nonce = [0_u8; 24];
    getrandom::fill(&mut dek).unwrap();
    getrandom::fill(&mut key_nonce).unwrap();
    getrandom::fill(&mut payload_nonce).unwrap();
    let kek_id = "local-test";
    let cipher = XChaCha20Poly1305::new(&Key::from(key));
    let mut wrapped = key_nonce.to_vec();
    wrapped.extend(
        cipher
            .encrypt(&XNonce::from(key_nonce), dek.as_slice())
            .unwrap(),
    );
    let ciphertext = XChaCha20Poly1305::new(&Key::from(dek))
        .encrypt(
            &XNonce::from(payload_nonce),
            Payload {
                msg: &serde_json::to_vec(value).unwrap(),
                aad: kek_id.as_bytes(),
            },
        )
        .unwrap();
    json!({
        "kek_id": kek_id,
        "wrapped_dek": base64::engine::general_purpose::STANDARD.encode(wrapped),
        "nonce": base64::engine::general_purpose::STANDARD.encode(payload_nonce),
        "ciphertext": base64::engine::general_purpose::STANDARD.encode(ciphertext),
    })
}

const V2_SCHEMA: &str = r#"
CREATE TABLE providers(id INTEGER PRIMARY KEY,name TEXT,channel TEXT,label TEXT,settings_json TEXT,credential_strategy TEXT,proxy_url TEXT,tls_fingerprint TEXT,enabled INTEGER);
CREATE TABLE credentials(id INTEGER PRIMARY KEY,provider_id INTEGER,name TEXT,kind TEXT,secret_json TEXT,weight INTEGER,rpm_limit INTEGER,tpm_limit INTEGER,proxy_url TEXT,tls_fingerprint TEXT,enabled INTEGER);
CREATE TABLE routes(id INTEGER PRIMARY KEY,name TEXT,enabled INTEGER);
CREATE TABLE route_members(id INTEGER PRIMARY KEY,route_id INTEGER,provider_id INTEGER,upstream_model_id TEXT,tier INTEGER,weight INTEGER,enabled INTEGER);
CREATE TABLE aliases(id INTEGER PRIMARY KEY,provider TEXT,alias TEXT,target TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE price_rules(id INTEGER PRIMARY KEY,provider_id INTEGER,match_type TEXT,model_match TEXT,input_price TEXT,output_price TEXT,cache_read_price TEXT,cache_creation_5m_price TEXT,cache_creation_30m_price TEXT,cache_creation_1h_price TEXT,image_output_price TEXT,pricing_tiers_json TEXT,enabled INTEGER);
CREATE TABLE price_rule_rates(id INTEGER PRIMARY KEY,price_rule_id INTEGER,metric TEXT,unit_size INTEGER,price_usd TEXT,conditions_json TEXT,sort_order INTEGER);
CREATE TABLE orgs(id INTEGER PRIMARY KEY,name TEXT,enabled INTEGER);
CREATE TABLE teams(id INTEGER PRIMARY KEY,org_id INTEGER,name TEXT,enabled INTEGER);
CREATE TABLE users(id INTEGER PRIMARY KEY,name TEXT,org_id INTEGER,team_id INTEGER,enabled INTEGER);
CREATE TABLE user_keys(id INTEGER PRIMARY KEY,user_id INTEGER,api_key_ciphertext TEXT,api_key_digest TEXT,api_key_digest_version INTEGER,label TEXT,enabled INTEGER);
CREATE TABLE quotas(id INTEGER PRIMARY KEY,scope TEXT,scope_id INTEGER,quota_total TEXT,quota_daily TEXT,quota_weekly TEXT,quota_monthly TEXT,quota_5h TEXT,quota_7d TEXT);
CREATE TABLE routing_rules(id INTEGER PRIMARY KEY,provider_id INTEGER,operation TEXT,kind TEXT,implementation TEXT,dest_operation TEXT,dest_kind TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE rule_sets(id INTEGER PRIMARY KEY,name TEXT,description TEXT,enabled INTEGER);
CREATE TABLE rules(id INTEGER PRIMARY KEY,rule_set_id INTEGER,kind TEXT,config_json TEXT,filter_model_pattern TEXT,filter_operation_keys TEXT,filter_header_pattern TEXT,sort_order INTEGER,enabled INTEGER);
CREATE TABLE provider_rule_sets(id INTEGER PRIMARY KEY,provider_id INTEGER,rule_set_id INTEGER,sort_order INTEGER,enabled INTEGER);
CREATE TABLE instance_settings(id INTEGER PRIMARY KEY,instance_name TEXT,proxy TEXT,spoof_emulation INTEGER,enable_usage INTEGER,enable_upstream_log INTEGER,enable_upstream_log_body INTEGER,enable_downstream_log INTEGER,enable_downstream_log_body INTEGER,disable_log_redaction INTEGER,enable_tokenizer_download INTEGER,retention_days INTEGER,max_database_size_mb INTEGER,file_upload_max_in_flight INTEGER);
CREATE TABLE usages(id INTEGER PRIMARY KEY,request_id TEXT,at INTEGER,route_name TEXT,provider_id INTEGER,credential_id INTEGER,org_id INTEGER,team_id INTEGER,user_id INTEGER,user_key_id INTEGER,thread_id TEXT,operation TEXT,kind TEXT,model TEXT,input_tokens INTEGER,output_tokens INTEGER,image_output_tokens INTEGER,cache_read_tokens INTEGER,cache_creation_5m_tokens INTEGER,cache_creation_30m_tokens INTEGER,cache_creation_1h_tokens INTEGER,metrics_json TEXT,cost TEXT,latency_ms INTEGER,usage_source TEXT,ended TEXT);
"#;
