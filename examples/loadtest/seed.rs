//! Bundle seeding + in-process `AppState` construction (mirrors
//! `src/pipeline/tests/mod.rs::build_state`, via public `gproxy::` paths).

use std::sync::Arc;

use gproxy::app::AppState;
use gproxy::app::snapshot::ControlPlaneSnapshot;
use gproxy::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use serde_json::{Value, json};

use crate::matrix::Wire;
use crate::mock::MockUpstream;

/// 4 providers (one per upstream wire), each with a route + global alias, one
/// org/user/api-key, and explicit content-generation routing rules pinning
/// every inbound wire to the provider's upstream wire. Usage recording and all
/// request logs are disabled via `instance_settings` to keep sqlite quiet.
fn bundle() -> String {
    let providers: Vec<(i64, Wire, &str, Value)> = vec![
        (
            1,
            Wire::Chat,
            "openai",
            json!({ "endpoints": {
                "openai_chat_completions": "http://chat.mock/v1/chat/completions"
            } }),
        ),
        (
            2,
            Wire::Responses,
            "openai",
            json!({ "endpoints": {
                "openai_responses": "http://resp.mock/v1/responses"
            } }),
        ),
        (
            3,
            Wire::Claude,
            "claudeapi",
            json!({ "endpoints": {
                "claude_messages": "http://cla.mock/v1/messages"
            } }),
        ),
        (
            4,
            Wire::Gemini,
            "aistudio",
            json!({ "endpoints": {
                "gemini_generate_content":
                    "http://gem.mock/v1beta/models/{model}:generateContent",
                "gemini_stream_generate_content":
                    "http://gem.mock/v1beta/models/{model}:streamGenerateContent"
            } }),
        ),
    ];

    let mut provider_rows = vec![];
    let mut credential_rows = vec![];
    let mut model_rows = vec![];
    let mut route_rows = vec![];
    let mut member_rows = vec![];
    let mut alias_rows = vec![];
    let mut rule_rows = vec![];
    let mut rule_id = 1i64;
    for (id, wire, channel, settings) in &providers {
        let name = format!("p-{}", wire.name());
        provider_rows.push(json!({
            "id": id, "name": name, "channel": channel, "label": null,
            "settings_json": settings, "credential_strategy": "round_robin",
            "proxy_url": null, "tls_fingerprint": null, "enabled": true
        }));
        credential_rows.push(json!({
            "id": id, "provider_id": id, "label": null,
            "secret_json": { "api_key": "up-key" }, "enabled": true
        }));
        model_rows.push(json!({
            "id": id, "provider_id": id, "model_id": "up-model",
            "display_name": null, "variants_json": null, "enabled": true
        }));
        route_rows.push(json!({
            "id": id, "name": format!("r-{}", wire.name()), "strategy": "failover",
            "enabled": true, "description": null
        }));
        member_rows.push(json!({
            "id": id, "route_id": id, "provider_id": id, "upstream_model_id": "up-model",
            "weight": 100, "tier": 0, "enabled": true
        }));
        alias_rows.push(json!({
            "id": id, "provider": "*", "alias": wire.alias(),
            "target": format!("r-{}", wire.name()), "sort_order": id, "enabled": true
        }));
        // Pin every inbound content wire to this provider's upstream wire.
        for op in ["generate_content", "stream_generate_content"] {
            for inbound in Wire::ALL {
                let (implementation, dest_op, dest_kind): (&str, Value, Value) = if inbound == *wire
                {
                    ("passthrough", Value::Null, Value::Null)
                } else {
                    ("transform_to", json!(op), json!(wire.kind_str()))
                };
                rule_rows.push(json!({
                    "id": rule_id, "provider_id": id, "operation": op,
                    "kind": inbound.kind_str(), "implementation": implementation,
                    "dest_operation": dest_op, "dest_kind": dest_kind,
                    "sort_order": rule_id, "enabled": true
                }));
                rule_id += 1;
            }
        }
    }

    json!({
        "schema_version": 1,
        "orgs": [{ "id": 1, "name": "default", "enabled": true, "description": null }],
        "users": [{ "id": 1, "name": "dev", "org_id": 1, "team_id": null,
                    "password": null, "enabled": true, "is_admin": false }],
        "user_keys": [{ "id": 1, "user_id": 1, "api_key": "sk-test",
                        "label": null, "enabled": true }],
        "route_permissions": [{ "id": 1, "scope": "user", "scope_id": 1,
                                "route_pattern": "*" }],
        "providers": provider_rows,
        "credentials": credential_rows,
        "provider_models": model_rows,
        "routes": route_rows,
        "route_members": member_rows,
        "aliases": alias_rows,
        "routing_rules": rule_rows,
        "instance_settings": [{
            "id": 1, "instance_name": "loadtest", "proxy": null, "spoof_emulation": null,
            "enable_usage": false, "enable_upstream_log": false,
            "enable_upstream_log_body": false, "enable_downstream_log": false,
            "enable_downstream_log_body": false, "disable_log_redaction": false,
            "update_channel": null
        }]
    })
    .to_string()
}

/// Import the bundle into `sqlite::memory:`, seed default routing (fill-missing,
/// explicit rules win) and assemble a serving `AppState`.
pub async fn build_state(mock: Arc<MockUpstream>) -> (Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let channels = Arc::new(gproxy::channel::registry::ChannelRegistry::with_builtin());
    let persistence: Arc<dyn gproxy::store::persistence::PersistenceBackend> = Arc::new(
        gproxy::store::persistence::DbPersistence::connect("sqlite::memory:")
            .await
            .expect("db persistence"),
    );
    gproxy::app::import::import_bundle(
        persistence.as_ref(),
        &gproxy::crypto::NoopCipher,
        &bundle(),
    )
    .await
    .expect("import bundle");
    for p in persistence.list_providers().await.expect("providers") {
        gproxy::api::routing::seed_default_routing(
            persistence.as_ref(),
            channels.as_ref(),
            p.id,
            false,
        )
        .await
        .expect("seed routing");
    }
    let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
        .await
        .expect("snapshot");
    let config = Arc::new(RuntimeConfig {
        host: "127.0.0.1".into(),
        port: 0,
        cache: CacheConfig::Memory,
        persistence: PersistenceConfig::Db {
            dsn: "sqlite::memory:".to_string(),
        },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts: gproxy::config::DEFAULT_MAX_ATTEMPTS,
        max_in_flight: gproxy::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_channel: "releases".to_string(),
        update_data_dir: dir.path().to_path_buf(),
        cors_origins: Vec::new(),
    });
    let cache: Arc<dyn gproxy::store::cache::CacheBackend> =
        Arc::new(gproxy::store::cache::MemoryCache::new());
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
    let state = AppState::new(
        config,
        cache,
        persistence,
        mock,
        snapshot,
        channels,
        Arc::new(gproxy::crypto::NoopCipher),
    );
    (Arc::new(state), dir)
}
