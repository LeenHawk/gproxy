use gproxy_core::{ExecutionLimits, LimitsError};
use gproxy_store::entity::config::setting;
use std::time::Duration;

fn settings() -> setting::Model {
    setting::Model {
        id: 1,
        instance_name: "default".into(),
        oauth_client_allowlist: None,
        connection_profile_id: None,
        cors_origins: serde_json::json!([]),
        trusted_proxies: serde_json::json!([]),
        max_attempts: 6,
        enable_settlement: true,
        enable_usage: true,
        config_revision: 0,
        request_timeout_ms: 600_000,
        stream_idle_timeout_ms: 60_000,
        max_request_body_bytes: 67_108_864,
        max_response_body_bytes: 67_108_864,
        max_stream_event_bytes: 1_048_576,
        max_ws_frame_bytes: 16_777_216,
        max_multipart_parts: 64,
        enable_tokenizer_vocabs: true,
        enable_tokenizer_download: false,
        default_vocabulary_file_id: None,
        tokenizer_auth_token: None,
        enable_downstream_log: true,
        enable_downstream_log_body: false,
        enable_upstream_log: true,
        enable_upstream_log_body: false,
        disable_log_redaction: false,
        enable_tracing: true,
        log_level: "info".into(),
        log_format: "text".into(),
        request_header_blacklist: serde_json::json!([]),
        response_header_blacklist: serde_json::json!([]),
        query_parameter_blacklist: serde_json::json!([]),
        retention_days: None,
        max_database_size_mb: None,
        update_channel: None,
        enable_auto_update_check: true,
        portal_recent_requests_enabled: true,
    }
}

#[test]
fn defaults_match_setting_defaults_and_deadline_only_shortens() {
    let limits = ExecutionLimits::from_settings(&settings()).unwrap();
    assert_eq!(limits, ExecutionLimits::default());
    let capability = limits.capability(Some(Duration::from_secs(5)));
    assert_eq!(capability.operation_total, Duration::from_secs(5));
    assert_eq!(
        limits
            .capability(Some(Duration::from_secs(9_999)))
            .operation_total,
        limits.request_timeout
    );
    assert_eq!(capability.read_bytes, 67_108_864);
    let codec = limits.codec();
    assert_eq!(codec.max_value_bytes, 1_048_576);
    assert_eq!(codec.max_parts, 64);
}

#[test]
fn zero_or_negative_limits_are_rejected() {
    let mut row = settings();
    row.max_response_body_bytes = 0;
    assert_eq!(
        ExecutionLimits::from_settings(&row),
        Err(LimitsError::NotPositive("max_response_body_bytes"))
    );
    let mut row = settings();
    row.request_timeout_ms = 0;
    assert_eq!(
        ExecutionLimits::from_settings(&row),
        Err(LimitsError::NotPositive("request_timeout_ms"))
    );
}
