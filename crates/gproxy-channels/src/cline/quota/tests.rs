use gproxy_channel_api::{Channel, QuotaSupport, QuotaValue};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use crate::ClineChannel;

#[test]
fn plan_query_reuses_api_key_without_user_identity_and_prefers_it_over_login() {
    for secret in [
        json!({"api_key": "manual"}),
        json!({"api_key": "manual", "access_token": "login"}),
    ] {
        let sources = ClineChannel.quota_sources(&secret, &json!({}));
        let source = sources
            .iter()
            .find(|source| source.id == "plan_usage")
            .unwrap();
        assert_eq!(source.support, QuotaSupport::Ready);
        assert!(source.automatic);
        let request = ClineChannel
            .prepare_quota_source("plan_usage", &secret, &json!({}))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), http::Method::GET);
        assert_eq!(
            request.uri(),
            "https://api.cline.bot/api/v1/users/me/plan/usage-limits"
        );
        assert_eq!(request.headers()["authorization"], "Bearer manual");
        assert_eq!(request.headers()["accept"], "application/json");
        assert!(request.body().is_empty());
    }
    let request = ClineChannel
        .prepare_quota_source(
            "plan_usage",
            &json!({"access_token": "login"}),
            &json!({"base_url": "https://cline.example/api/v1/"}),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        request.uri(),
        "https://cline.example/api/v1/users/me/plan/usage-limits"
    );
    assert_eq!(request.headers()["authorization"], "Bearer workos:login");
    assert!(
        ClineChannel
            .prepare_quota_source("plan_usage", &json!({}), &json!({}))
            .unwrap()
            .is_none()
    );
}

#[test]
fn plan_windows_preserve_percentages_and_upstream_resets_without_inventing_totals() {
    let raw = json!({"success": true, "data": {"limits": [
        {"type": "five_hour", "percentUsed": 0.5, "resetsAt": "2030-01-01T05:00:00Z"},
        {"type": "weekly", "percentUsed": 62, "resetsAt": null},
        {"type": "monthly", "percentUsed": 100, "resetsAt": "2030-02-01T08:00:00+08:00"},
        {"type": "future_window", "percentUsed": 105.5}
    ]}});
    let entries = parse(&raw).unwrap();
    assert_eq!(entries.len(), 4);
    for (entry, (kind, percent, reset)) in entries.iter().zip([
        ("five_hour", "0.5", Some("2030-01-01T05:00:00Z")),
        ("weekly", "62", None),
        ("monthly", "100", Some("2030-02-01T00:00:00Z")),
        ("future_window", "105.5", None),
    ]) {
        assert_eq!(entry.id, kind);
        assert_eq!(entry.source_id, "plan_usage");
        let QuotaValue::Window(window) = &entry.value else {
            panic!("expected plan window")
        };
        assert_eq!(window.used_percent, Some(percent.parse().unwrap()));
        assert_eq!(
            window.period_end,
            reset.and_then(crate::shared::quota::iso_to_unix)
        );
        assert_eq!(
            (window.used, window.limit, window.remaining),
            (None, None, None)
        );
        assert!(window.period_start.is_none());
        assert!(window.unit.is_none());
    }
    assert_eq!(entries[3].label.as_deref(), Some("future_window"));
}

#[test]
fn invalid_or_absent_plan_results_fail_instead_of_replacing_saved_usage() {
    for raw in [
        json!({"success": false, "data": {"limits": []}}),
        json!({"success": true, "data": null}),
        json!({"success": true, "data": {}}),
        json!({"success": true, "data": {"limits": []}}),
    ] {
        assert!(parse(&raw).is_err());
    }
    for limit in [
        json!({"type": "five_hour"}),
        json!({"type": "five_hour", "percentUsed": "50"}),
        json!({"type": "five_hour", "percentUsed": -1}),
        json!({"percentUsed": 50}),
        json!({"type": "five_hour", "percentUsed": 50, "resetsAt": "invalid"}),
        json!({"type": "five_hour", "percentUsed": 50, "resetsAt": 123}),
    ] {
        assert!(parse(&json!({"success": true, "data": {"limits": [limit]}})).is_err());
    }
    let limit = json!({"type": "five_hour", "percentUsed": 50});
    assert!(parse(&json!({"success": true, "data": {"limits": [limit, limit]}})).is_err());
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::NOT_FOUND,
    ] {
        let error = ClineChannel
            .parse_quota_source(
                "plan_usage",
                status,
                &HeaderMap::new(),
                b"private upstream error",
            )
            .unwrap_err();
        assert!(error.to_string().contains(&status.to_string()));
        assert!(!error.to_string().contains("private upstream error"));
    }
}

fn parse(
    raw: &Value,
) -> Result<Vec<gproxy_channel_api::QuotaEntry>, gproxy_channel_api::ChannelError> {
    ClineChannel.parse_quota_source(
        "plan_usage",
        StatusCode::OK,
        &HeaderMap::new(),
        &serde_json::to_vec(raw).unwrap(),
    )
}
