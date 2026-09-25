//! Configuration management: what a write costs, what it refuses, and what
//! the instance serves afterwards.
//!
//! The contract under test is not "CRUD works". It is that one write is one
//! revision, that a refused write leaves no trace, that a secret never becomes
//! readable by accident, and that the narrow credential path stays narrow.

mod support;

use std::sync::Arc;

use gproxy_sdk::{
    ClientPool, CredentialStatus, Gproxy, GproxyBuilder, RefreshMode, SdkError, SyncMode,
    dto::{
        BatchItem, BatchPatch, ConnectionProfileWrite, CredentialPatch, CredentialWrite, ListQuery,
        LoggingSettingsPatch, ModelWrite, OperationEndpointWrite, OperationRuleWrite,
        PriceRateWrite, PriceRuleWrite, PriceTierWrite, ProviderDto, ProviderModelWrite,
        ProviderPatch, ProviderRuleSetWrite, ProviderWrite, QuotaWrite, RewriteRuleWrite,
        RouteMemberWrite, RouteWrite, RuleSetWrite, SettingsPatch,
    },
};
use gproxy_store::entity::limits::credential_block;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::json;

/// The durable revision, which is what a write actually advances. The active
/// snapshot only follows it when the write asked for a full reload.
async fn durable(gproxy: &Gproxy<DatabaseConnection>) -> i64 {
    gproxy
        .store()
        .settings()
        .get()
        .await
        .unwrap()
        .unwrap()
        .config_revision
}

async fn provider(gproxy: &Gproxy<DatabaseConnection>, name: &str) -> ProviderDto {
    gproxy
        .manage()
        .providers()
        .create(ProviderWrite {
            name: name.to_owned(),
            channel: "test".to_owned(),
            base_url: Some("https://upstream.example/v1".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
}

async fn credential(
    gproxy: &Gproxy<DatabaseConnection>,
    provider_id: &str,
    secret: &str,
) -> String {
    gproxy
        .manage()
        .credentials()
        .create(CredentialWrite {
            provider_id: provider_id.to_owned(),
            label: Some("primary".to_owned()),
            auth_kind: "api_key".to_owned(),
            secret: json!({ "api_key": secret }),
            ..Default::default()
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn every_family_round_trips() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let manage = gproxy.manage();

    // providers
    let provider = provider(&gproxy, "upstream").await;
    assert_eq!(
        manage.providers().get(&provider.id).await.unwrap().name,
        "upstream"
    );
    let updated = manage
        .providers()
        .update(
            &provider.id,
            ProviderPatch {
                enabled: Some(false),
                base_url: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.base_url, None, "an explicit null clears the column");

    // connection profiles
    let profile = manage
        .connection_profiles()
        .create(ConnectionProfileWrite {
            name: "direct".to_owned(),
            backend: Some("reqwest".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        manage
            .connection_profiles()
            .get(&profile.id)
            .await
            .unwrap()
            .backend,
        "reqwest"
    );
    manage
        .connection_profiles()
        .delete(&profile.id)
        .await
        .unwrap();
    assert!(matches!(
        manage.connection_profiles().get(&profile.id).await,
        Err(SdkError::NotFound { .. })
    ));

    // catalog
    let model = manage
        .models()
        .create(ModelWrite {
            name: "m1".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let provider_model = manage
        .provider_models()
        .create(ProviderModelWrite {
            provider_id: provider.id.clone(),
            upstream_name: "m1-upstream".to_owned(),
            model_id: Some(model.id.clone()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(provider_model.model_id.as_deref(), Some(model.id.as_str()));

    // routing
    let route = manage
        .routes()
        .create(RouteWrite {
            name: "fast".to_owned(),
            strategy: Some("weighted".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(route.strategy, "weighted");
    let member = manage
        .route_members()
        .create(RouteMemberWrite {
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            upstream_model: "m1-upstream".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(member.weight, 100);

    // endpoints
    let rule = manage
        .endpoints()
        .operation_rules()
        .create(OperationRuleWrite {
            provider_id: provider.id.clone(),
            operation: "generate_content".to_owned(),
            action: "deny".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let endpoint = manage
        .endpoints()
        .operation_endpoints()
        .create(OperationEndpointWrite {
            provider_id: provider.id.clone(),
            operation: "generate_content".to_owned(),
            dialect: "openai".to_owned(),
            url: "https://upstream.example/v1/responses".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(endpoint.transport, "http");

    // rewrite
    let set = manage
        .rewrite()
        .sets()
        .create(RuleSetWrite {
            name: "redactions".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let rewritten = manage
        .rewrite()
        .replace_rules(
            &set.id,
            vec![
                RewriteRuleWrite {
                    pattern: "secret".to_owned(),
                    replacement: "[redacted]".to_owned(),
                    ..Default::default()
                },
                RewriteRuleWrite {
                    pattern: "token".to_owned(),
                    replacement: "[redacted]".to_owned(),
                    ..Default::default()
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(rewritten.len(), 2);
    assert_eq!(rewritten[0].sort_order, 0);
    assert_eq!(rewritten[1].sort_order, 1, "caller order is rule order");
    let binding = manage
        .rewrite()
        .bindings()
        .create(ProviderRuleSetWrite {
            provider_id: provider.id.clone(),
            rule_set_id: set.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(binding.enabled);

    // quotas
    let quota = manage
        .quotas()
        .create(QuotaWrite {
            owner_kind: "user".to_owned(),
            owner_id: "u1".to_owned(),
            metric: "cost".to_owned(),
            unit: "USD".to_owned(),
            limit_value: "25".to_owned(),
            period: "1d".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(quota.window_key, "primary");
    assert_eq!(quota.limit_value, "25");

    // pricing
    let price_rule = manage
        .pricing()
        .rules()
        .create(PriceRuleWrite {
            model_pattern: "m1*".to_owned(),
            currency: "usd".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(price_rule.currency, "USD");
    let rate = manage
        .pricing()
        .rates()
        .create(PriceRateWrite {
            price_rule_id: price_rule.id.clone(),
            metric: "input_tokens".to_owned(),
            unit: "token".to_owned(),
            unit_quantity: "1000000".to_owned(),
            value: "3".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(rate.value, "3");
    let tier = manage
        .pricing()
        .tiers()
        .create(PriceTierWrite {
            price_rule_id: price_rule.id.clone(),
            min_prompt_tokens: Some(200_000),
            input_per_million: Some("6".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(tier.input_per_million.as_deref(), Some("6"));

    // credentials
    let credential_id = credential(&gproxy, &provider.id, "k-1").await;
    assert!(
        manage
            .credentials()
            .get(&credential_id)
            .await
            .unwrap()
            .has_secret
    );

    // Deleting a parent takes its children with it, so the leaves go first.
    manage.pricing().tiers().delete(&tier.id).await.unwrap();
    manage.pricing().rates().delete(&rate.id).await.unwrap();
    manage
        .pricing()
        .rules()
        .delete(&price_rule.id)
        .await
        .unwrap();
    manage.quotas().delete(&quota.id).await.unwrap();
    manage
        .rewrite()
        .bindings()
        .delete(&binding.id)
        .await
        .unwrap();
    manage
        .rewrite()
        .rules()
        .delete(&rewritten[0].id)
        .await
        .unwrap();
    manage.rewrite().sets().delete(&set.id).await.unwrap();
    manage
        .endpoints()
        .operation_endpoints()
        .delete(&endpoint.id)
        .await
        .unwrap();
    manage
        .endpoints()
        .operation_rules()
        .delete(&rule.id)
        .await
        .unwrap();
    manage.route_members().delete(&member.id).await.unwrap();
    manage.routes().delete(&route.id).await.unwrap();
    manage
        .provider_models()
        .delete(&provider_model.id)
        .await
        .unwrap();
    manage.models().delete(&model.id).await.unwrap();
    manage.credentials().delete(&credential_id).await.unwrap();
    manage.providers().delete(&provider.id).await.unwrap();

    assert_eq!(
        manage
            .providers()
            .list(ListQuery::default())
            .await
            .unwrap()
            .total,
        0
    );
}

#[tokio::test]
async fn one_write_is_one_revision() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let before = durable(&gproxy).await;

    let provider = provider(&gproxy, "upstream").await;
    assert_eq!(durable(&gproxy).await, before + 1);
    assert_eq!(gproxy.revision().0, u64::try_from(before + 1).unwrap());

    gproxy
        .manage()
        .providers()
        .update(
            &provider.id,
            ProviderPatch {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(durable(&gproxy).await, before + 2);

    // A whole batch is still one revision, however many rows it names.
    gproxy
        .manage()
        .models()
        .batch(vec![
            BatchItem::Create(ModelWrite {
                name: "a".to_owned(),
                ..Default::default()
            }),
            BatchItem::Create(ModelWrite {
                name: "b".to_owned(),
                ..Default::default()
            }),
        ])
        .await
        .unwrap();
    assert_eq!(durable(&gproxy).await, before + 3);
}

#[tokio::test]
async fn a_rejected_write_costs_nothing() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    let route = gproxy
        .manage()
        .routes()
        .create(RouteWrite {
            name: "fast".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let set = gproxy
        .manage()
        .rewrite()
        .sets()
        .create(RuleSetWrite {
            name: "broken".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let settled = durable(&gproxy).await;
    let manage = gproxy.manage();

    let unknown_channel = manage
        .providers()
        .create(ProviderWrite {
            name: "other".to_owned(),
            channel: "nope".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("a provider cannot name a channel this build has no code for");
    assert!(
        matches!(unknown_channel, SdkError::Invalid(_)),
        "{unknown_channel}"
    );

    let duplicate = manage
        .providers()
        .create(ProviderWrite {
            name: "upstream".to_owned(),
            channel: "test".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("provider names are unique");
    assert!(matches!(duplicate, SdkError::Conflict(_)), "{duplicate}");
    assert_eq!(duplicate.status_code(), 409);

    let reserved = manage
        .routes()
        .create(RouteWrite {
            name: "test/foo".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("`test` is a registered channel id, so `test/…` already means narrowing");
    assert!(matches!(reserved, SdkError::Invalid(_)), "{reserved}");
    let provider_prefixed = manage
        .routes()
        .create(RouteWrite {
            name: "upstream/foo".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("a provider name is reserved the same way");
    assert!(matches!(provider_prefixed, SdkError::Invalid(_)));

    let zero_weight = manage
        .route_members()
        .create(RouteMemberWrite {
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            upstream_model: "m1".to_owned(),
            weight: Some(0),
            ..Default::default()
        })
        .await
        .expect_err("a zero weight is a member that is never chosen");
    assert!(matches!(zero_weight, SdkError::Invalid(_)));

    let bad_url = manage
        .providers()
        .create(ProviderWrite {
            name: "relative".to_owned(),
            channel: "test".to_owned(),
            base_url: Some("upstream.example".to_owned()),
            ..Default::default()
        })
        .await
        .expect_err("a base URL must be absolute");
    assert!(matches!(bad_url, SdkError::Invalid(_)));

    let bad_config = manage
        .providers()
        .create(ProviderWrite {
            name: "listy".to_owned(),
            channel: "test".to_owned(),
            config: Some(json!([1, 2, 3])),
            ..Default::default()
        })
        .await
        .expect_err("provider config is an object of channel keys");
    assert!(matches!(bad_config, SdkError::Invalid(_)));

    let missing_provider = manage
        .route_members()
        .create(RouteMemberWrite {
            route_id: route.id.clone(),
            provider_id: "ghost".to_owned(),
            upstream_model: "m1".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("a member names a provider that exists");
    assert!(matches!(missing_provider, SdkError::NotFound { .. }));
    assert_eq!(missing_provider.status_code(), 404);

    let bad_quota = manage
        .quotas()
        .create(QuotaWrite {
            owner_kind: "user".to_owned(),
            owner_id: "u1".to_owned(),
            metric: "bananas".to_owned(),
            unit: "USD".to_owned(),
            limit_value: "1".to_owned(),
            period: "1d".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("a quota neither a budget nor a limit would compile is refused");
    assert!(matches!(bad_quota, SdkError::Invalid(_)), "{bad_quota}");

    let bad_rule = manage
        .rewrite()
        .rules()
        .create(RewriteRuleWrite {
            rule_set_id: Some(set.id.clone()),
            pattern: "([unclosed".to_owned(),
            replacement: "x".to_owned(),
            ..Default::default()
        })
        .await
        .expect_err("a rule that cannot compile would silently never run");
    assert!(matches!(bad_rule, SdkError::Invalid(_)), "{bad_rule}");

    assert_eq!(
        durable(&gproxy).await,
        settled,
        "nothing that was refused advanced the revision"
    );
}

#[tokio::test]
async fn a_secret_is_sealed_and_revealed_only_on_request() {
    let gproxy = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .master_key([7u8; 32])
        .without_default_channels()
        .channel(Arc::new(support::TestChannel::default()))
        .client_pool(ClientPool::with_client(Arc::new(
            support::ScriptClient::default(),
        )))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    let provider = provider(&gproxy, "upstream").await;
    let secret = json!({ "api_key": "super-secret-token" });
    let created = gproxy
        .manage()
        .credentials()
        .create(CredentialWrite {
            provider_id: provider.id.clone(),
            auth_kind: "api_key".to_owned(),
            secret: secret.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(created.has_secret);

    let row = gproxy
        .store()
        .credentials()
        .get_many(std::slice::from_ref(&created.id))
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    let plaintext = serde_json::to_vec(&secret).unwrap();
    assert_ne!(row.secret, plaintext);
    assert!(
        !row.secret
            .windows(b"super-secret-token".len())
            .any(|window| window == b"super-secret-token"),
        "the sealed column must not contain the token"
    );

    assert_eq!(
        gproxy
            .manage()
            .credentials()
            .reveal_secret(&created.id)
            .await
            .unwrap(),
        secret
    );

    // A new secret reseals and bumps the version, which is what makes a peer
    // re-read the row.
    let rotated = gproxy
        .manage()
        .credentials()
        .update(
            &created.id,
            CredentialPatch {
                secret: Some(json!({ "api_key": "rotated" })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(rotated.version, created.version + 1);
    assert_eq!(
        gproxy
            .manage()
            .credentials()
            .reveal_secret(&created.id)
            .await
            .unwrap(),
        json!({ "api_key": "rotated" })
    );
}

#[tokio::test]
async fn credential_state_writes_keep_the_snapshot() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    let credential_id = credential(&gproxy, &provider.id, "k-1").await;
    let before = gproxy.core().snapshot();
    let durable_before = durable(&gproxy).await;

    gproxy
        .manage()
        .credentials()
        .update(
            &credential_id,
            CredentialPatch {
                secret: Some(json!({ "api_key": "k-2" })),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(
        durable(&gproxy).await,
        durable_before + 1,
        "it is still a write"
    );
    assert!(
        Arc::ptr_eq(&before, &gproxy.core().snapshot()),
        "a state-only credential write re-reads one row, it does not rebuild providers"
    );
    assert_eq!(
        before
            .credentials
            .get(&credential_id)
            .unwrap()
            .state
            .load()
            .version,
        1,
        "the live credential slot did pick the new version up"
    );

    // Touching anything the snapshot froze is a full reload instead.
    gproxy
        .manage()
        .credentials()
        .update(
            &credential_id,
            CredentialPatch {
                label: Some(Some("renamed".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&before, &gproxy.core().snapshot()));
}

#[tokio::test]
async fn health_reset_clears_blocks_and_revives_the_credential() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    let credential_id = credential(&gproxy, &provider.id, "k-1").await;

    // A block only counts while it is in force, so it has to be dated from the
    // same clock the read filters on.
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    gproxy
        .store()
        .credential_blocks()
        .create_many(vec![credential_block::ActiveModel {
            id: Set("block-1".to_owned()),
            credential_id: Set(credential_id.clone()),
            scope: Set(json!("all")),
            operation: Set(None),
            until_ms: Set(now + 86_400_000),
            source: Set(json!({ "kind": "operator" })),
            observed_at_ms: Set(now),
        }])
        .await
        .unwrap();

    let dead = gproxy
        .manage()
        .credentials()
        .set_status(
            &credential_id,
            CredentialStatus::Dead,
            Some("operator".to_owned()),
        )
        .await
        .unwrap();
    assert_eq!(dead.status, "dead");
    assert_eq!(dead.status_reason.as_deref(), Some("operator"));
    assert!(
        !gproxy
            .manage()
            .credentials()
            .quota_read(&credential_id)
            .await
            .unwrap()
            .blocks
            .is_empty()
    );

    let revived = gproxy
        .manage()
        .credentials()
        .health_reset(&credential_id)
        .await
        .unwrap();
    assert_eq!(revived.status, "active");
    assert_eq!(revived.status_reason, None);
    assert_eq!(
        gproxy
            .store()
            .credential_blocks()
            .query(
                credential_block::Entity::find()
                    .filter(credential_block::Column::CredentialId.eq(&credential_id))
            )
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn reset_routing_defaults_touches_one_provider() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let kept = provider(&gproxy, "kept").await;
    let cleared = provider(&gproxy, "cleared").await;
    let manage = gproxy.manage();

    for provider in [&kept, &cleared] {
        manage
            .endpoints()
            .operation_rules()
            .create(OperationRuleWrite {
                provider_id: provider.id.clone(),
                operation: "generate_content".to_owned(),
                action: "deny".to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
        manage
            .endpoints()
            .operation_endpoints()
            .create(OperationEndpointWrite {
                provider_id: provider.id.clone(),
                operation: "generate_content".to_owned(),
                dialect: "openai".to_owned(),
                url: "https://upstream.example/v1/responses".to_owned(),
                ..Default::default()
            })
            .await
            .unwrap();
    }

    let before = durable(&gproxy).await;
    manage
        .providers()
        .reset_routing_defaults(&cleared.id)
        .await
        .unwrap();
    assert_eq!(
        durable(&gproxy).await,
        before + 1,
        "both deletes are one commit"
    );

    let rules = manage
        .endpoints()
        .operation_rules()
        .list(ListQuery::default())
        .await
        .unwrap();
    assert_eq!(rules.total, 1);
    assert_eq!(rules.items[0].provider_id, kept.id);
    let endpoints = manage
        .endpoints()
        .operation_endpoints()
        .list(ListQuery::default())
        .await
        .unwrap();
    assert_eq!(endpoints.total, 1);
    assert_eq!(endpoints.items[0].provider_id, kept.id);
}

#[tokio::test]
async fn a_settings_write_reaches_the_active_snapshot() {
    let (gproxy, _, _) = support::sdk_parts().await;
    assert!(gproxy.core().snapshot().observation.upstream_log);

    let settings = gproxy
        .manage()
        .settings()
        .update(SettingsPatch {
            logging: Some(LoggingSettingsPatch {
                enable_upstream_log: Some(false),
                disable_log_redaction: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!settings.logging.enable_upstream_log);

    let snapshot = gproxy.core().snapshot();
    assert!(
        !snapshot.observation.upstream_log,
        "the reload that follows a settings write is what makes it take effect"
    );
    assert!(!snapshot.observation.redact);
    assert_eq!(
        u64::try_from(settings.instance.config_revision).unwrap(),
        snapshot.revision.0,
        "the row reports the very revision it was written at"
    );

    // One bump per write: `commit_revision` already advances it, and the
    // settings statement must not do so a second time.
    let before = durable(&gproxy).await;
    gproxy
        .manage()
        .settings()
        .update(SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                instance_name: Some("edge-1".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(durable(&gproxy).await, before + 1);
    assert_eq!(
        gproxy
            .manage()
            .settings()
            .get()
            .await
            .unwrap()
            .instance
            .instance_name,
        "edge-1"
    );
}

#[tokio::test]
async fn a_batch_is_one_commit_over_mixed_steps() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let manage = gproxy.manage();
    let doomed = manage
        .models()
        .create(ModelWrite {
            name: "doomed".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();
    let kept = manage
        .models()
        .create(ModelWrite {
            name: "kept".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let before = durable(&gproxy).await;
    let out = manage
        .models()
        .batch(vec![
            BatchItem::Create(ModelWrite {
                name: "fresh".to_owned(),
                ..Default::default()
            }),
            BatchItem::Update(BatchPatch {
                id: kept.id.clone(),
                patch: gproxy_sdk::dto::ModelPatch {
                    metadata: Some(json!({ "family": "x" })),
                    ..Default::default()
                },
            }),
            BatchItem::Delete(doomed.id.clone()),
        ])
        .await
        .unwrap();
    assert_eq!(durable(&gproxy).await, before + 1);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].as_ref().unwrap().name, "fresh");
    assert_eq!(out[1].as_ref().unwrap().metadata, json!({ "family": "x" }));
    assert!(out[2].is_none(), "a delete has no row to return");
    assert!(matches!(
        manage.models().get(&doomed.id).await,
        Err(SdkError::NotFound { .. })
    ));
}

#[tokio::test]
async fn account_operations_reach_the_channel() {
    let (gproxy, channel, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    let credential_id = credential(&gproxy, &provider.id, "k-1").await;

    channel
        .refreshes
        .lock()
        .unwrap()
        .push_back(support::RefreshReply::Rotated {
            api_key: "k-2",
            expires_at_ms: None,
        });
    let summary = gproxy
        .manage()
        .credentials()
        .refresh(&credential_id, RefreshMode::Force)
        .await
        .unwrap();
    assert_eq!(summary.credential_id, credential_id);
    assert_eq!(summary.status, "active");

    channel
        .quota_snapshots
        .lock()
        .unwrap()
        .push_back(gproxy_channel::channel::QuotaSnapshot {
            observed_at_ms: 1,
            entries: Vec::new(),
        });
    let probed = gproxy
        .manage()
        .credentials()
        .quota_probe(&credential_id)
        .await
        .unwrap();
    assert!(probed.entries.is_empty());

    // The test channel reports usage but cannot reopen a window, and saying so
    // is better than pretending the reset happened.
    let unsupported = gproxy
        .manage()
        .credentials()
        .quota_reset(&credential_id)
        .await
        .expect_err("this channel implements no QuotaReset");
    assert!(
        matches!(unsupported, SdkError::Unsupported(_)),
        "{unsupported}"
    );
    assert_eq!(unsupported.status_code(), 501);

    assert!(
        gproxy
            .manage()
            .credentials()
            .limit_status(&credential_id)
            .await
            .unwrap()
            .is_empty(),
        "no operator limit covers this credential yet"
    );
}

#[tokio::test]
async fn a_credential_limit_is_visible_on_its_credential() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    let credential_id = credential(&gproxy, &provider.id, "k-1").await;

    gproxy
        .manage()
        .quotas()
        .create(QuotaWrite {
            owner_kind: "credential".to_owned(),
            owner_id: credential_id.clone(),
            window_key: Some("daily".to_owned()),
            metric: "requests".to_owned(),
            unit: "count".to_owned(),
            limit_value: "100".to_owned(),
            period: "1d".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap();

    let status = gproxy
        .manage()
        .quotas()
        .limit_status(&credential_id)
        .await
        .unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].window_key, "daily");
    assert_eq!(status[0].limit, "100");
    assert_eq!(status[0].used, "0", "a limit nothing charged opens no row");
    assert_eq!(status[0].owner_kind, "credential");

    let budgets = gproxy
        .manage()
        .quotas()
        .budget_status(&[gproxy_sdk::BudgetOwner::new("credential", &credential_id)])
        .await
        .unwrap();
    assert!(
        budgets.is_empty(),
        "an operator limit is not a caller budget, whatever its owner kind looks like"
    );
}

#[tokio::test]
async fn a_credential_list_narrows_by_owner() {
    // The three owner columns are opaque to this crate — a host decides what
    // they mean — but the list has to be narrowable by them, or a multi-tenant
    // host would have to filter pages after the fact and report the unfiltered
    // counts.
    let (gproxy, _, _) = support::sdk_parts().await;
    let provider = provider(&gproxy, "upstream").await;
    // The owner columns carry foreign keys, so the rows they name have to
    // exist even though nothing in this crate reads them.
    gproxy
        .store()
        .users()
        .create_many(vec![gproxy_store::entity::identity::user::ActiveModel {
            id: Set("alice".to_owned()),
            name: Set("alice".to_owned()),
            role: Set("user".to_owned()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    gproxy
        .store()
        .organizations()
        .create_many(vec![
            gproxy_store::entity::identity::organization::ActiveModel {
                id: Set("acme".to_owned()),
                name: Set("acme".to_owned()),
                created_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    gproxy
        .store()
        .teams()
        .create_many(vec![gproxy_store::entity::identity::team::ActiveModel {
            id: Set("core".to_owned()),
            organization_id: Set("acme".to_owned()),
            name: Set("core".to_owned()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    for (id, user, team, org) in [
        ("c-shared", None, None, None),
        ("c-alice", Some("alice"), None, None),
        ("c-core", None, Some("core"), None),
        ("c-acme", None, None, Some("acme")),
    ] {
        gproxy
            .manage()
            .credentials()
            .create(CredentialWrite {
                id: Some(id.to_owned()),
                provider_id: provider.id.clone(),
                auth_kind: "api_key".to_owned(),
                secret: json!({ "api_key": "k" }),
                user_id: user.map(str::to_owned),
                team_id: team.map(str::to_owned),
                organization_id: org.map(str::to_owned),
                ..Default::default()
            })
            .await
            .unwrap();
    }

    async fn listed(
        gproxy: &Gproxy<DatabaseConnection>,
        owner_kind: Option<&str>,
        owner_id: Option<&str>,
    ) -> Vec<String> {
        let mut ids: Vec<String> = gproxy
            .manage()
            .credentials()
            .list(ListQuery {
                owner_kind: owner_kind.map(str::to_owned),
                owner_id: owner_id.map(str::to_owned),
                ..Default::default()
            })
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|item| item.id)
            .collect();
        ids.sort();
        ids
    }

    assert_eq!(
        listed(&gproxy, None, None).await,
        ["c-acme", "c-alice", "c-core", "c-shared"],
        "no owner filter is no narrowing"
    );
    assert_eq!(listed(&gproxy, Some("org"), Some("acme")).await, ["c-acme"]);
    assert_eq!(
        listed(&gproxy, Some("team"), Some("core")).await,
        ["c-core"]
    );
    assert_eq!(
        listed(&gproxy, Some("user"), Some("alice")).await,
        ["c-alice"]
    );
    // `instance` is the rows with no owner at all, which is what a
    // single-tenant deployment's credentials look like.
    assert_eq!(
        listed(&gproxy, Some("instance"), None).await,
        ["c-shared".to_owned()]
    );
    // A kind with no id is "every row owned by one of these".
    assert_eq!(listed(&gproxy, Some("org"), None).await, ["c-acme"]);
    // An owner this table cannot hold matches nothing rather than everything.
    assert!(
        listed(&gproxy, Some("api_key"), Some("k1"))
            .await
            .is_empty()
    );
    assert!(
        listed(&gproxy, Some("provider"), Some("p1"))
            .await
            .is_empty()
    );
    assert!(
        listed(&gproxy, Some("org"), Some("globex"))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn provider_display_name_is_independent_from_its_invocation_name() {
    let (gproxy, _, _) = support::sdk_parts().await;
    let original = gproxy
        .manage()
        .providers()
        .create(ProviderWrite {
            name: "route-name".into(),
            display_name: Some("显示名称".into()),
            channel: "test".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let changed = gproxy
        .manage()
        .providers()
        .update(
            &original.id,
            ProviderPatch {
                display_name: Some(Some("新显示名称".into())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(changed.name, "route-name");
    assert_eq!(changed.display_name.as_deref(), Some("新显示名称"));
    let by_label = gproxy
        .manage()
        .providers()
        .list(ListQuery {
            search: Some("新显示".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(by_label.items[0].id, original.id);
    let changed = gproxy
        .manage()
        .providers()
        .update(
            &original.id,
            ProviderPatch {
                name: Some("new-route".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(changed.display_name.as_deref(), Some("新显示名称"));
    let cleared = gproxy
        .manage()
        .providers()
        .update(
            &original.id,
            ProviderPatch {
                display_name: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(cleared.name, "new-route");
    assert_eq!(cleared.display_name, None);
}

#[test]
fn update_payloads_do_not_accept_creation_only_types() {
    for value in [json!({"channel": "test"}), json!({"channel": null})] {
        assert!(serde_json::from_value::<ProviderPatch>(value.clone()).is_err());
        assert!(
            serde_json::from_value::<BatchItem<ProviderWrite, ProviderPatch>>(
                json!({"update": {"id": "p", "patch": value}})
            )
            .is_err()
        );
    }
    for value in [json!({"authKind": "oauth"}), json!({"authKind": null})] {
        assert!(serde_json::from_value::<CredentialPatch>(value.clone()).is_err());
        assert!(
            serde_json::from_value::<BatchItem<CredentialWrite, CredentialPatch>>(
                json!({"update": {"id": "c", "patch": value}})
            )
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<ProviderPatch>(
            json!({"displayName": "Friendly name", "name": "route"})
        )
        .is_ok()
    );
    assert!(
        serde_json::from_value::<CredentialPatch>(json!({"label": "Friendly credential"})).is_ok()
    );
}
