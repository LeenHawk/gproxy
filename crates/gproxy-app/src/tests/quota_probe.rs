use super::setup;
use crate::{ControlMutation, MutationResult};
use gproxy_admin::State;
use gproxy_channel_api::QuotaValue;
use serde_json::json;
use std::io::{Read, Write};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn quota_snapshot_reads_without_egress_and_refresh_preserves_last_success() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let server = std::thread::spawn(move || {
        for status in [
            "200 OK",
            "401 Unauthorized",
            "200 OK",
            "429 Too Many Requests",
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut data = [0_u8; 4096];
            let n = stream.read(&mut data).unwrap();
            assert!(
                std::str::from_utf8(&data[..n])
                    .unwrap()
                    .starts_with("GET /user/balance ")
            );
            seen.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}]}"#;
            let retry = if status.starts_with("429") {
                "Retry-After: 3600\r\n"
            } else {
                ""
            };
            write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{retry}\r\n{body}",body.len()).unwrap();
        }
    });
    let fixture = setup::fixture().await;
    let app = &fixture.app;
    let MutationResult::Id(provider) = app
        .mutate(ControlMutation::Provider(
            gproxy_store::records::ProviderInput {
                name: "quota source".into(),
                label: None,
                channel: "deepseek".into(),
                settings: json!({"base_url":format!("http://{address}/v1")}),
                credential_strategy: "round_robin".into(),
                proxy_url: None,
                tls_fingerprint: None,
                enabled: true,
            },
        ))
        .await
        .unwrap()
    else {
        panic!("provider")
    };
    let MutationResult::Id(id) = app
        .mutate(ControlMutation::Credential {
            provider_id: provider,
            label: None,
            secret: json!({"api_key":setup::random_key()}),
            enabled: true,
        })
        .await
        .unwrap()
    else {
        panic!("credential")
    };
    let initial = app.credential_quota_snapshot(id).await.unwrap();
    assert!(initial.entries.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let first = app.quota_probe(id, false).await.unwrap();
    let QuotaValue::Balance(value) = &first.snapshot.entries[0].value else {
        panic!("balance")
    };
    assert_eq!(value.remaining.unwrap(), 110.into());
    let observed = first.snapshot.sources[0].observed_at_ms;
    assert!(
        app.quota_probe(id, false).await.unwrap().snapshot.sources[0]
            .error
            .is_none()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let failed = app.quota_probe(id, true).await.unwrap();
    assert!(
        failed.snapshot.sources[0]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("401")
    );
    assert_eq!(failed.snapshot.entries, first.snapshot.entries);
    assert_eq!(failed.snapshot.sources[0].observed_at_ms, observed);
    let saved = app.credential_quota_snapshot(id).await.unwrap();
    assert_eq!(saved, failed.snapshot);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        app.quota_probe(id, true).await.unwrap().snapshot.sources[0]
            .error
            .is_none()
    );
    let limited = app.quota_probe(id, true).await.unwrap();
    assert_eq!(
        limited.snapshot.sources[0].error.as_ref().unwrap().code,
        "rate_limited"
    );
    assert!(
        app.quota_probe(id, true).await.unwrap().snapshot.sources[0]
            .error
            .is_some()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert!(
        app.store()
            .credential_quota_cycles(Some(id), 0, crate::quota_refresh::now() + 1)
            .await
            .unwrap()
            .is_empty()
    );
    server.join().unwrap();
}
