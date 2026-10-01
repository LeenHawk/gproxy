#![cfg(not(target_arch = "wasm32"))]

mod support;
use support::*;

use gproxy_channel::channels::cline::Cline;
use gproxy_core::{Core, CoreError, PlaintextCodec};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_store::entity::{
    config::setting,
    upstream::{credential, provider},
};
use http::StatusCode;
use sea_orm::Set;
use serde_json::json;
use std::sync::Arc;

async fn cline() -> Harness {
    let mut h = harness(full(), "sticky").await;
    h.core
        .store()
        .providers()
        .update_many(vec![provider::ActiveModel {
            id: Set("p".into()),
            channel: Set("cline".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    h.core
        .store()
        .credentials()
        .update_many(vec![credential::ActiveModel {
            id: Set("a".into()),
            metadata: Set(json!({"user_id": "upstream-user"})),
            ..Default::default()
        }])
        .await
        .unwrap();
    h.core = Core::builder(h.core.store().clone())
        .cache(Arc::new(gproxy_cache::MemoryCache::default()))
        .observer(h.observer.clone())
        .secret_codec(Arc::new(PlaintextCodec))
        .client_pool(gproxy_client::ClientPool::with_client(h.client.clone()))
        .channel(h.channel.clone())
        .unwrap()
        .channel(Arc::new(Cline))
        .unwrap()
        .build()
        .unwrap();
    h.core.reload_data().await.unwrap();
    h
}

async fn probe(h: &Harness, used: u32, balance: u32) {
    h.script(vec![
        json_reply(
            StatusCode::OK,
            json!({"success":true,"data":{"limits":[{
                "type":"five_hour","percentUsed":used,"resetsAt":"2099-01-01T00:00:00Z"
            }]}}),
        ),
        json_reply(
            StatusCode::OK,
            json!({"success":true,"data":{"balance":balance}}),
        ),
        json_reply(
            StatusCode::OK,
            json!({"free":[{"id":"stealth/space-bunny-alpha"}]}),
        ),
    ]);
    h.core.query_credential_quota("p", "a").await.unwrap();
}

async fn usable(h: &Harness, model: &str) -> bool {
    let mut ctx = h.context_for(
        "p",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        model,
        1,
        None,
    );
    let ctx_mut = Arc::make_mut(&mut ctx);
    ctx_mut.target.credentials.truncate(1);
    ctx_mut.target.upstream_model = Some(model.into());
    h.script(vec![json_reply(StatusCode::OK, json!({"ok":true}))]);
    match h.core.stream_generate_content(ctx, request("{}")).await {
        Ok(execution) => {
            let (response, completion) = execution.into_parts();
            read(response.body).await;
            completion.await.unwrap();
            true
        }
        Err(CoreError::NoUsableCredential) => false,
        Err(error) => panic!("unexpected error: {error}"),
    }
}

async fn permission(h: &Harness, allow: bool) {
    h.core
        .store()
        .credentials()
        .update_many(vec![credential::ActiveModel {
            id: Set("a".into()),
            metadata: Set(json!({"user_id":"upstream-user","allow_paid_usage":allow})),
            ..Default::default()
        }])
        .await
        .unwrap();
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set((h.core.snapshot().revision.0 + 1) as i64),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

#[tokio::test]
async fn zero_credits_never_block_clinepass_or_free_models_even_after_repeated_probes() {
    let h = cline().await;
    for _ in 0..2 {
        probe(&h, 4, 0).await;
        let blocks = h
            .core
            .store()
            .load_control_data()
            .await
            .unwrap()
            .credential_blocks;
        assert!(
            blocks
                .iter()
                .any(|b| b.source["dimension"] == "cline_balance"
                    && b.scope["except_models"] == json!(["stealth/space-bunny-alpha"]))
        );
        assert!(usable(&h, "cline-pass/deepseek-v4.1-flash").await);
        assert!(usable(&h, "cline-free/kat-coder-pro").await);
        assert!(usable(&h, "arcee-ai/trinity:free").await);
        assert!(usable(&h, "stealth/space-bunny-alpha").await);
        assert!(!usable(&h, "anthropic/claude-fable").await);
    }
    // Legacy rows cannot override the newer account scope when reloaded.
    h.core
        .store()
        .credential_blocks()
        .create_many(vec![
            gproxy_store::entity::limits::credential_block::ActiveModel {
                id: Set("legacy-balance".into()),
                credential_id: Set("a".into()),
                source: Set(
                    json!({"kind":"quota_exhausted","dimension":"cline_balance","cycle_id":null}),
                ),
                scope: Set(json!("all")),
                operation: Set(None),
                until_ms: Set(i64::MAX),
                observed_at_ms: Set(1),
            },
        ])
        .await
        .unwrap();
    permission(&h, false).await;
    assert!(usable(&h, "cline-pass/deepseek-v4.1-flash").await);
    assert!(usable(&h, "stealth/space-bunny-alpha").await);
    // A positive reading clears prior scopes even if the catalog has changed.
    probe(&h, 4, 10).await;
    assert!(usable(&h, "anthropic/claude-fable").await);
}

#[test]
fn a_new_free_model_list_replaces_older_balance_scopes_in_any_load_order() {
    use gproxy_channel::channel::QuotaScope;
    use gproxy_core::{BlockSource, CredentialBlock, CredentialBlocks};
    let block = |scope, observed_at_ms| CredentialBlock {
        scope,
        observed_at_ms,
        operation: None,
        until_ms: i64::MAX,
        source: BlockSource::QuotaExhausted {
            dimension: "cline_balance".into(),
            cycle_id: None,
        },
    };
    for reverse in [false, true] {
        let mut rows = vec![
            block(QuotaScope::All, 1),
            block(QuotaScope::ExceptModels(vec!["old-free".into()]), 2),
            block(QuotaScope::ExceptModels(vec!["new-free".into()]), 3),
        ];
        if reverse {
            rows.reverse();
        }
        let mut blocks = CredentialBlocks::default();
        for row in rows {
            blocks.upsert(row, 0);
        }
        assert_eq!(blocks.blocks.len(), 1);
        assert!(
            blocks
                .blocked_by(Some("new-free"), Operation::GenerateContent, 0)
                .is_none()
        );
        assert!(
            blocks
                .blocked_by(Some("old-free"), Operation::GenerateContent, 0)
                .is_some()
        );
    }
}

#[tokio::test]
async fn paid_usage_toggle_reuses_persisted_exhaustion_without_clearing_limits() {
    let h = cline().await;
    probe(&h, 100, 20).await;
    assert!(!usable(&h, "cline-pass/deepseek-v4.1-flash").await);
    assert!(usable(&h, "cline-free/kat-coder-pro").await);
    assert!(usable(&h, "anthropic/claude-fable").await);
    permission(&h, true).await;
    assert!(usable(&h, "cline-pass/deepseek-v4.1-flash").await);
    permission(&h, false).await;
    assert!(!usable(&h, "cline-pass/deepseek-v4.1-flash").await);
    assert!(
        !h.core
            .store()
            .load_control_data()
            .await
            .unwrap()
            .credential_blocks
            .is_empty()
    );
}

#[tokio::test]
async fn paid_usage_permission_never_bypasses_upstream_cooldowns_or_local_limits() {
    let h = cline().await;
    permission(&h, true).await;
    for source in [
        gproxy_core::BlockSource::RateLimited,
        gproxy_core::BlockSource::Counted {
            dimension: "operator_limit".into(),
        },
    ] {
        let blocks = gproxy_core::CredentialBlocks {
            blocks: vec![gproxy_core::CredentialBlock {
                source,
                scope: gproxy_channel::channel::QuotaScope::All,
                operation: None,
                until_ms: i64::MAX,
                observed_at_ms: 1,
            }],
            ..Default::default()
        };
        h.core
            .cache()
            .put(
                &gproxy_core::keys::credential_blocks("p", "a"),
                serde_json::to_vec(&blocks).unwrap(),
                std::time::Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert!(!usable(&h, "cline-pass/deepseek-v4.1-flash").await);
    }
}
