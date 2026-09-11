use serde_json::json;

use crate::backend::{DbValue, Statement};
use crate::records::{UsageAggregateQuery, UsageFilter, UsageGroupBy};
use crate::{Store, StoreError};

pub(super) async fn run(store: &Store) -> Result<(), StoreError> {
    let filter = UsageFilter {
        from: 0,
        to: 4_000,
        ..Default::default()
    };
    let query = UsageAggregateQuery {
        from: filter.from,
        to: filter.to,
        group_by: UsageGroupBy::Dimensions,
        user_key_id: None,
        user_id: None,
        provider_id: None,
        credential_id: None,
        model: None,
    };
    let original = store.usage_by_request("request-1").await?.unwrap();
    let totals = store.usage_summary(&filter).await?;
    let statistics = store.usage_aggregate(&query).await?;
    let legacy = json!({
        "dimensions": {"tier": "legacy", "operation": "generate_content"},
        "quantities": {
            "audio_seconds": "1",
            "cache_creation_5m_tokens": "999",
            "cache_creation_30m_tokens": "4",
            "cache_creation_1h_tokens": 5
        },
        "cache_creation_5m_tokens": "3"
    });
    write_metrics(store, &legacy).await?;
    assert_eq!(store.usage_summary(&filter).await?, totals);
    assert_eq!(store.usage_aggregate(&query).await?, statistics);
    let detail = store.usage_by_request("request-1").await?.unwrap();
    assert_eq!(detail.usage.dimensions["tier"], "standard");
    assert_eq!(detail.usage.dimensions["operation"], "generate_content");
    assert_eq!(detail.usage.metrics["cache_creation_5m_tokens"], "3");
    assert!(detail.usage.metrics.get("quantities").is_none());
    assert!(detail.usage.metrics.get("dimensions").is_none());
    let (records, count) = store.usage_records(&filter, 1, 10).await?;
    assert_eq!(count, 2);
    assert_eq!(
        records.iter().find(|row| row.id == detail.id),
        Some(&detail)
    );

    let old_only = UsageFilter {
        request_id: Some("request-1".into()),
        ..filter.clone()
    };
    assert_eq!(store.usage_summary(&old_only).await?.requests, 1);
    assert_eq!(
        store.usage_summary(&old_only).await?.total_tokens(),
        27.into()
    );

    for malformed in [
        json!({"quantities": {"audio_seconds": "invalid"}}),
        json!({"quantities": {"audio_seconds": {"nested": 1}}}),
    ] {
        write_metrics(store, &malformed).await?;
        assert!(matches!(
            store.usage_summary(&filter).await,
            Err(StoreError::InvalidData {
                field: "metrics_json",
                ..
            })
        ));
    }
    write_metrics(store, &original.usage.metrics).await?;
    Ok(())
}

async fn write_metrics(store: &Store, metrics: &serde_json::Value) -> Result<(), StoreError> {
    store
        .backend()
        .execute(Statement::with_args(
            "UPDATE usage_rows SET metrics_json = ? WHERE request_id = ?",
            vec![
                DbValue::Text(metrics.to_string()),
                DbValue::Text("request-1".into()),
            ],
        ))
        .await?;
    Ok(())
}
