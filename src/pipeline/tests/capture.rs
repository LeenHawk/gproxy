use super::*;

#[tokio::test]
async fn buffered_error_response_body_is_captured() {
    let error_body = Bytes::from_static(
        br#"{"error":{"code":"content_rejected","message":"request rejected"}}"#,
    );
    let mut upstream = FakeUpstream::new(error_body.clone(), vec![]);
    upstream.statuses = vec![StatusCode::FORBIDDEN];
    let upstream = Arc::new(upstream);
    let bundle = bundle_with(
        "instance_settings",
        json!([{
            "id": 1,
            "instance_name": "test",
            "proxy": null,
            "spoof_emulation": false,
            "enable_usage": false,
            "enable_upstream_log": true,
            "enable_upstream_log_body": true,
            "enable_downstream_log": false,
            "enable_downstream_log_body": false,
            "disable_log_redaction": false,
            "enable_tokenizer_download": false,
            "update_channel": null
        }]),
    );
    let (state, _dir) = state_with_bundle(upstream, &bundle).await;

    let result = crate::pipeline::execute(&state, claude_ctx("claude-test", false)).await;
    assert!(result.is_err(), "AuthDead 403 should fail the request");

    let mut captured = None;
    for _ in 0..100 {
        let rows = state
            .persistence
            .list_upstream_requests("t-1")
            .await
            .unwrap();
        if !rows.is_empty() {
            captured = Some(rows);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let rows = captured.expect("buffered upstream capture did not finish");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, 403);
    assert_eq!(
        rows[0].response_body.as_deref(),
        Some(r#"{"error":{"code":"[REDACTED]","message":"request rejected"}}"#)
    );
}
