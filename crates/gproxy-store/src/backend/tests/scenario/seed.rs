use rust_decimal::Decimal;
use serde_json::json;

use crate::backend::Statement;
use crate::records::*;
use crate::{Store, StoreError};

pub(super) async fn seed_identity(store: &Store) -> Result<i64, StoreError> {
    let organization = store
        .insert_organization(&OrganizationInput {
            name: "org".into(),
            enabled: true,
        })
        .await?;
    let team = store
        .insert_team(&TeamInput {
            organization_id: organization,
            name: "team".into(),
            enabled: true,
        })
        .await?;
    let user = store
        .insert_user(&UserInput {
            name: "user".into(),
            organization_id: Some(organization),
            team_id: Some(team),
            password_hash: None,
            enabled: true,
            is_admin: false,
        })
        .await?;
    let key = store
        .insert_user_key(&UserKeyInput {
            user_id: user,
            digest: vec![7; 32],
            digest_version: 1,
            prefix: "prefix".into(),
            envelope: envelope(7),
            label: None,
            expires_at: None,
            enabled: true,
        })
        .await?;
    store
        .insert_permission(&PermissionInput {
            subject_kind: "user_key".into(),
            subject_id: key,
            provider_id: None,
            operation_group: None,
            allowed: true,
        })
        .await?;
    store
        .insert_rate_limit(&RateLimitInput {
            subject_kind: "user_key".into(),
            subject_id: key,
            requests: 10,
            window_seconds: 60,
        })
        .await?;
    store
        .insert_quota(&QuotaInput {
            subject_kind: "user_key".into(),
            subject_id: key,
            quota_total: Some(Decimal::from(1_000)),
            quota_daily: Some(Decimal::from(100)),
            quota_weekly: None,
            quota_monthly: None,
            quota_5h: None,
            quota_7d: None,
            enabled: true,
        })
        .await?;
    Ok(key)
}

pub(super) async fn seed_pricing(store: &Store, provider_id: i64) -> Result<(), StoreError> {
    let rule = store
        .insert_price_rule(&PriceRuleInput {
            provider_id: Some(provider_id),
            model_pattern: "upstream-model".into(),
            tiers: Some(json!([{
                "service_tier": "priority",
                "input_price": "2",
                "output_price": "4"
            }])),
            priority: 0,
            enabled: true,
        })
        .await?;
    for (metric, price) in [("input_tokens", 1), ("output_tokens", 2)] {
        store
            .insert_price_rate(&PriceRateInput {
                rule_id: rule,
                metric: metric.into(),
                unit_size: 1_000_000,
                price: Decimal::from(price),
                conditions: None,
                priority: 0,
            })
            .await?;
    }
    Ok(())
}

pub(super) fn usage(provider_id: i64, credential_id: i64) -> UsageInput {
    UsageInput {
        upstream_started_at_ms: Some(3_601_000),
        request_id: "request-1".into(),
        at: 3_601,
        provider_id,
        credential_id,
        organization_id: Some(1),
        team_id: Some(1),
        user_id: Some(1),
        user_key_id: Some(1),
        operation: Some("generate_content".into()),
        upstream_model: "upstream-model".into(),
        input_tokens: 10,
        output_tokens: 5,
        cached_input_tokens: 2,
        metrics: json!({
            "audio_seconds": 1,
            "cache_creation_5m_tokens": 3,
            "cache_creation_30m_tokens": 4,
            "cache_creation_1h_tokens": 5
        }),
        dimensions: json!({"tier": "standard"}),
        cost: Decimal::new(2, 5),
        usage_source: "upstream".into(),
        ended: "complete".into(),
        latency_ms: 12,
    }
}

pub(super) async fn seed_binding(
    store: &Store,
    provider_id: i64,
    credential_id: i64,
) -> Result<BindingPage, StoreError> {
    store
        .save_binding(&BindingInput {
            provider_id,
            owner_user_id: 1,
            kind: "file".into(),
            resource_id: "file-1".into(),
            credential_id,
            summary: json!({"id": "file-1"}),
        })
        .await?;
    store.list_bindings(provider_id, 1, "file", None, 10).await
}

pub(super) async fn seed_capture(
    store: &Store,
    provider_id: i64,
    credential_id: i64,
) -> Result<(), StoreError> {
    store
        .begin_request_log(&RequestLogInput {
            request_id: "request-1".into(),
            at: 3_601,
            method: "POST".into(),
            path: "/v1/responses".into(),
            query: None,
            client_ip: Some("198.51.100.7".into()),
            request_headers: None,
            request_body: None,
        })
        .await?;
    store
        .record_capture(&capture(provider_id, credential_id, 3_601, 503))
        .await?;
    store
        .record_capture(&capture(provider_id, credential_id, 3_602, 200))
        .await?;
    store
        .finish_request_log(&RequestLogCompletion {
            request_id: "request-1".into(),
            response_status: 200,
            error_kind: None,
            response_headers: None,
            response_body: Some(b"response".to_vec()),
        })
        .await?;
    let archived = RequestLogImportInput {
        request: RequestLogInput {
            request_id: "imported-request".into(),
            at: 0,
            method: "POST".into(),
            path: "/v1/messages".into(),
            query: Some("stream=true".into()),
            client_ip: None,
            request_headers: Some(json!({"content-type":"application/json"})),
            request_body: Some(b"request\0body".to_vec()),
        },
        response_status: None,
        response_body: Some("响应".as_bytes().to_vec()),
    };
    store
        .insert_record_batch(RecordBatch::RequestLogs(vec![archived.clone()]))
        .await?;
    let mut wire = capture(provider_id, credential_id, 0, 503);
    wire.request_id = archived.request.request_id.clone();
    store
        .insert_record_batch(RecordBatch::Captures(vec![wire.clone()]))
        .await?;
    let detail = store
        .log_detail(&archived.request.request_id)
        .await?
        .expect("imported log");
    assert_eq!(detail.downstream.input, archived.request);
    assert_eq!(detail.downstream.response_status, archived.response_status);
    assert_eq!(detail.downstream.response_body, archived.response_body);
    assert_eq!(detail.upstream.len(), 1);
    assert_eq!(detail.upstream[0].input, wire);
    Ok(())
}

fn capture(provider_id: i64, credential_id: i64, at: i64, status: u16) -> CaptureInput {
    CaptureInput {
        request_id: "request-1".into(),
        at,
        provider_id: Some(provider_id),
        credential_id: Some(credential_id),
        upstream_url: Some("https://upstream.invalid/v1/responses".into()),
        request_method: Some("POST".into()),
        request_headers: None,
        response_status: Some(status),
        response_headers: None,
        request_body: Some(b"request".to_vec()),
        response_body: Some(b"response".to_vec()),
    }
}

pub(super) async fn scalar(store: &Store, sql: &str) -> Result<i64, StoreError> {
    let mut result = store.backend().execute(Statement::plain(sql)).await?;
    let row = result
        .rows
        .pop()
        .ok_or_else(|| StoreError::Database("scalar row missing".into()))?;
    row.i64(if sql.contains("COUNT") {
        "value"
    } else {
        "requests"
    })
}

pub(super) fn envelope(byte: u8) -> CredentialEnvelope {
    CredentialEnvelope {
        ciphertext: vec![byte; 8],
        wrapped_key: vec![byte; 8],
        payload_nonce: vec![byte; 12],
        key_nonce: vec![byte; 12],
    }
}
