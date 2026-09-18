#![cfg(not(target_arch = "wasm32"))]

use gproxy_channel::{
    BaseChannel,
    channel::{
        CredentialView, ProviderView, QuotaDimension, QuotaMetric, QuotaModel, QuotaScope,
        QuotaTracking, QuotaWindow,
    },
};
use gproxy_core::{
    CapturePolicy, CaptureSink, ConfigRevision, Core, CredentialStatus, CredentialStrategy,
    EndpointTransport, ExchangeContext, ObservationPolicy, Observer, PlaintextCodec,
    RequestContext, SecretCodec, TraceEvent, UsageReport, keys,
};
use gproxy_protocol::{Dialect, Operation, OperationKey, capability::CapabilityFuture};
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        limits::credential_block,
        upstream::{
            credential, operation_endpoint, provider, provider_rewrite_rule_set, rewrite_rule,
            rewrite_rule_set,
        },
    },
    operations::credentials::CredentialRefresh,
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;
use std::sync::Arc;

struct ObserveNothing;
impl Observer for ObserveNothing {
    fn policy(&self, _: &RequestContext) -> ObservationPolicy {
        ObservationPolicy {
            usage: false,
            capture: CapturePolicy::Off,
            trace: false,
        }
    }
    fn capture(&self, _: &ExchangeContext, _: CapturePolicy) -> Box<dyn CaptureSink> {
        unreachable!()
    }
    fn usage<'a>(&'a self, _: &'a UsageReport) -> CapabilityFuture<'a, ()> {
        Box::pin(async {})
    }
    fn trace(&self, _: TraceEvent<'_>) {}
}

struct TestChannel;
impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
}
impl QuotaModel for TestChannel {
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let plan = credential.secret["plan"].as_str().unwrap_or("free");
        vec![QuotaDimension {
            id: format!("daily_{plan}"),
            label: None,
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Requests,
            window: QuotaWindow::CalendarDay,
            limit: Some(50.into()),
            tracking: QuotaTracking::Counted,
        }]
    }
}

async fn database() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    Store::new(db)
}

fn credential_row(id: &str, provider: &str, secret: serde_json::Value) -> credential::ActiveModel {
    credential::ActiveModel {
        id: Set(id.into()),
        provider_id: Set(provider.into()),
        user_id: Set(Some("u".into())),
        auth_kind: Set("api_key".into()),
        secret: Set(PlaintextCodec.seal(id, &secret).unwrap()),
        metadata: Set(json!({})),
        ..Default::default()
    }
}

async fn seed(store: &Store<DatabaseConnection>) {
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(3),
            max_response_body_bytes: Set(1024),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![gproxy_store::entity::identity::user::ActiveModel {
            id: Set("u".into()),
            name: Set("u".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .providers()
        .create_many(vec![
            provider::ActiveModel {
                id: Set("p1".into()),
                name: Set("p1".into()),
                channel: Set("test".into()),
                base_url: Set(Some("https://api.example".into())),
                config: Set(json!({"credential_strategy": "sticky", "vendor": {"x": 1}})),
                created_at_ms: Set(0),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("disabled".into()),
                name: Set("disabled".into()),
                channel: Set("nope".into()),
                config: Set(json!({})),
                enabled: Set(false),
                created_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .credentials()
        .create_many(vec![
            credential_row("c1", "p1", json!({"api_key": "k1", "plan": "pro"})),
            credential_row("c2", "p1", json!({"api_key": "k2"})),
            credential_row("orphan", "disabled", json!({"api_key": "k3"})),
        ])
        .await
        .unwrap();
    store
        .operation_endpoints()
        .create_many(vec![
            operation_endpoint::ActiveModel {
                id: Set("e1".into()),
                provider_id: Set("p1".into()),
                operation: Set("generate_content".into()),
                dialect: Set("openai".into()),
                url: Set("https://alt.example/v1/responses".into()),
                ..Default::default()
            },
            operation_endpoint::ActiveModel {
                id: Set("e-off".into()),
                provider_id: Set("p1".into()),
                operation: Set("list_models".into()),
                dialect: Set("openai".into()),
                url: Set("https://alt.example/v1/models".into()),
                enabled: Set(false),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .rewrite_rule_sets()
        .create_many(vec![rewrite_rule_set::ActiveModel {
            id: Set("set".into()),
            name: Set("set".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .rewrite_rules()
        .create_many(vec![rewrite_rule::ActiveModel {
            id: Set("r1".into()),
            rule_set_id: Set("set".into()),
            paths: Set(Some(json!(["tools.*.name"]))),
            pattern: Set("^mcp_(.*)$".into()),
            replacement: Set("mcp__$1".into()),
            filter_model_pattern: Set(Some("claude-*".into())),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .provider_rewrite_rule_sets()
        .create_many(vec![provider_rewrite_rule_set::ActiveModel {
            id: Set("bind".into()),
            provider_id: Set("p1".into()),
            rule_set_id: Set("set".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let far = i64::MAX / 2;
    store
        .credential_blocks()
        .create_many(vec![
            credential_block::ActiveModel {
                id: Set("live".into()),
                credential_id: Set("c1".into()),
                scope: Set(json!({"model_prefixes": ["claude-sonnet-4"]})),
                operation: Set(None),
                until_ms: Set(far),
                source: Set(json!({"kind": "quota_exhausted", "dimension": "seven_day_sonnet", "cycle_id": null})),
                observed_at_ms: Set(1),
            },
            credential_block::ActiveModel {
                id: Set("expired".into()),
                credential_id: Set("c1".into()),
                scope: Set(json!("all")),
                operation: Set(None),
                until_ms: Set(1),
                source: Set(json!({"kind": "rate_limited"})),
                observed_at_ms: Set(1),
            },
        ])
        .await
        .unwrap();
}

async fn core(store: Store<DatabaseConnection>) -> Core<DatabaseConnection> {
    Core::builder(Arc::new(store))
        .cache(Arc::new(gproxy_cache::MemoryCache::default()))
        .observer(Arc::new(ObserveNothing))
        .secret_codec(Arc::new(PlaintextCodec))
        .channel(Arc::new(TestChannel))
        .unwrap()
        .build()
        .unwrap()
}

#[tokio::test]
async fn load_assembles_providers_credentials_rules_endpoints_and_warms_blocks() {
    let store = database().await;
    seed(&store).await;
    let core = core(store).await;
    let outcome = core.reload_data().await.unwrap();
    assert_eq!(outcome.loaded_revision, ConfigRevision(3));
    assert!(outcome.published);
    let data = core.snapshot();
    assert_eq!(data.limits.max_response_body_bytes, 1024);
    assert_eq!(data.providers.len(), 1);
    let p1 = &data.providers["p1"];
    assert_eq!(p1.channel.id(), "test");
    assert_eq!(p1.credential_strategy, CredentialStrategy::Sticky);
    assert_eq!(p1.credential_ids, ["c1", "c2"]);
    assert_eq!(
        p1.operation_url(
            OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAi
            },
            EndpointTransport::Http
        ),
        Some("https://alt.example/v1/responses")
    );
    assert_eq!(
        p1.operation_url(
            OperationKey {
                operation: Operation::ListModels,
                dialect: Dialect::OpenAi
            },
            EndpointTransport::Http
        ),
        None
    );
    assert_eq!(p1.rewrite_rule_sets.len(), 1);
    let set = &data.rewrite_rule_sets["set"];
    assert_eq!(set.rules.len(), 1);
    assert!(
        set.rules[0]
            .model_matcher
            .as_ref()
            .unwrap()
            .is_match("claude-x")
    );
    assert!(!set.rules[0].model_matcher.as_ref().unwrap().is_match("gpt"));

    assert_eq!(data.credentials.len(), 2);
    let c1 = &data.credentials["c1"];
    assert_eq!(c1.quota[0].id, "daily_pro");
    assert_eq!(c1.state.load().secret["api_key"], "k1");
    assert_eq!(c1.state.load().status, CredentialStatus::Active);
    assert!(!Arc::ptr_eq(&c1.client, &c1.websocket_client));
    let c2 = &data.credentials["c2"];
    assert!(
        Arc::ptr_eq(&c1.client, &c2.client),
        "same profile shares one client"
    );

    let cached = core
        .cache()
        .get(&keys::credential_blocks("p1", "c1"))
        .await
        .unwrap()
        .expect("blocks warmed");
    let blocks: gproxy_core::CredentialBlocks = serde_json::from_slice(&cached.value).unwrap();
    assert_eq!(blocks.blocks.len(), 1, "expired row is not warmed");
    assert!(
        blocks
            .blocked_by(Some("claude-sonnet-4-5"), Operation::GenerateContent, 2)
            .is_some()
    );
}

#[tokio::test]
async fn reload_reuses_credential_slots_and_a_bad_rule_keeps_the_old_snapshot() {
    let store = database().await;
    seed(&store).await;
    let core = core(store).await;
    core.reload_data().await.unwrap();
    let first = core.snapshot();
    let slot = first.credentials["c1"].state.clone();

    core.store()
        .credentials()
        .refresh_many(vec![CredentialRefresh {
            id: "c1".into(),
            expected_version: 0,
            secret: PlaintextCodec
                .seal("c1", &json!({"api_key": "k1-new"}))
                .unwrap(),
            expires_at_ms: Some(99),
        }])
        .await
        .unwrap();
    core.store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(4),
            ..Default::default()
        })
        .await
        .unwrap();
    let outcome = core.reload_data().await.unwrap();
    assert!(outcome.published);
    let second = core.snapshot();
    assert!(Arc::ptr_eq(&slot, &second.credentials["c1"].state));
    assert_eq!(slot.load().version, 1);
    assert_eq!(slot.load().secret["api_key"], "k1-new");

    core.store()
        .rewrite_rules()
        .create_many(vec![rewrite_rule::ActiveModel {
            id: Set("bad".into()),
            rule_set_id: Set("set".into()),
            pattern: Set("(".into()),
            replacement: Set("".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    core.store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(5),
            ..Default::default()
        })
        .await
        .unwrap();
    let error = core.reload_data().await.unwrap_err();
    assert!(error.to_string().contains("rewrite rule `bad`"), "{error}");
    assert_eq!(core.snapshot().revision, ConfigRevision(4));
}

#[tokio::test]
async fn reload_credentials_publishes_material_and_retires_deleted_rows() {
    let store = database().await;
    seed(&store).await;
    let core = core(store).await;
    core.reload_data().await.unwrap();
    let data = core.snapshot();
    core.store()
        .credentials()
        .refresh_many(vec![CredentialRefresh {
            id: "c2".into(),
            expected_version: 0,
            secret: PlaintextCodec
                .seal("c2", &json!({"api_key": "rotated"}))
                .unwrap(),
            expires_at_ms: None,
        }])
        .await
        .unwrap();
    core.store()
        .credentials()
        .delete_many(&["c1".into()])
        .await
        .unwrap();
    let summaries = core
        .reload_credentials(&["c2".into(), "c1".into(), "missing".into(), "c2".into()])
        .await
        .unwrap();
    assert_eq!(summaries[0].as_ref().unwrap().version, 1);
    assert!(summaries[1].is_none());
    assert!(summaries[2].is_none());
    assert_eq!(summaries[3].as_ref().unwrap().version, 1);
    assert_eq!(
        data.credentials["c2"].state.load().secret["api_key"],
        "rotated"
    );
    assert!(data.credentials["c1"].state.is_retired());
    assert!(!data.credentials["c2"].state.is_retired());
}
