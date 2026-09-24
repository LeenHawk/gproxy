use super::{block_on, memory::MemoryHost, target};
use crate::{Core, CoreError};
use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelRegistry, QuotaValue};
use http::StatusCode;
use serde_json::json;

const SUMMARY: &[u8] = br#"{"groups":[{"displayName":"Gemini Models","buckets":[{"bucketId":"gemini-5h","remainingFraction":0.5,"resetTime":"2026-09-15T12:00:00Z"}]}]}"#;

fn antigravity_fixture(expiry: i64) -> (MemoryHost, Core<MemoryHost>, crate::ProviderRef) {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "antigravity".into();
        state.credential.secret = json!({
            "access_token":"old", "refresh_token":"refresh-fixture", "project_id":"test-project",
            "expires_at_ms":expiry,
        });
    }
    let core = Core::new(
        host.clone(),
        ChannelRegistry::new([Box::new(gproxy_channels::AntigravityChannel) as Box<dyn Channel>])
            .unwrap(),
    )
    .unwrap();
    let mut provider = target().provider;
    provider.channel = "antigravity".into();
    provider.settings = json!({});
    (host, core, provider)
}

#[test]
fn quota_probe_refreshes_expired_oauth_and_returns_the_rotated_version() {
    let (host, core, provider) = antigravity_fixture(1);
    host.state.lock().unwrap().scripted.extend([
        (
            StatusCode::OK,
            vec![Bytes::from_static(
                br#"{"access_token":"fresh","expires_in":3600}"#,
            )],
        ),
        (StatusCode::OK, vec![Bytes::from_static(SUMMARY)]),
    ]);
    let result =
        block_on(core.quota_source(&provider, crate::CredentialId(7), 4, "subscription")).unwrap();
    assert_eq!(result.credential_version, 5);
    assert_eq!(result.entries.len(), 1);
    let state = host.state.lock().unwrap();
    assert_eq!(state.rotations, [4]);
    assert_eq!(state.upstream_requests.len(), 2);
    assert_eq!(
        state.upstream_requests[0].1,
        "https://oauth2.googleapis.com/token"
    );
    assert_eq!(
        state.upstream_requests[1].0[http::header::AUTHORIZATION],
        "Bearer fresh"
    );
    assert!(
        state.upstream_requests[1]
            .1
            .ends_with(":retrieveUserQuotaSummary")
    );
    assert!(state.settlements.is_empty());
    assert_eq!(state.admit_calls, 0);
}

#[test]
fn quota_probe_retries_401_once_but_not_403_or_429() {
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::TOO_MANY_REQUESTS,
    ] {
        let (host, core, provider) = antigravity_fixture(i64::MAX);
        host.state.lock().unwrap().scripted.extend([
            (status, vec![Bytes::from_static(b"denied")]),
            (
                StatusCode::OK,
                vec![Bytes::from_static(
                    br#"{"access_token":"fresh","expires_in":3600}"#,
                )],
            ),
            (
                StatusCode::UNAUTHORIZED,
                vec![Bytes::from_static(b"still denied")],
            ),
        ]);
        assert!(
            block_on(core.quota_probe("antigravity", &provider, crate::CredentialId(7))).is_err()
        );
        let state = host.state.lock().unwrap();
        assert_eq!(
            state.upstream_requests.len(),
            if status == StatusCode::UNAUTHORIZED {
                3
            } else {
                1
            }
        );
        assert_eq!(
            state.rotations.len(),
            usize::from(status == StatusCode::UNAUTHORIZED)
        );
    }
}

#[test]
fn quota_probe_legacy_fallback_is_bounded_and_does_not_refresh_a_valid_token() {
    for status in [StatusCode::NOT_FOUND, StatusCode::OK] {
        let (host, core, provider) = antigravity_fixture(i64::MAX);
        host.state.lock().unwrap().scripted.extend([
            (status, vec![Bytes::from_static(b"{}")]),
            (
                StatusCode::OK,
                vec![Bytes::from_static(
                    br#"{"models":{"gemini-pro-agent":{"quotaInfo":{"remainingFraction":0.8}}}}"#,
                )],
            ),
        ]);
        let result =
            block_on(core.quota_probe("antigravity", &provider, crate::CredentialId(7))).unwrap();
        assert_eq!(result.credential_version, 4);
        assert_eq!(result.observations.len(), 1);
        let state = host.state.lock().unwrap();
        assert!(state.rotations.is_empty());
        assert_eq!(state.upstream_requests.len(), 2);
        assert!(
            state.upstream_requests[1]
                .1
                .ends_with(":fetchAvailableModels")
        );
    }
}

#[test]
fn quota_probe_rejects_stale_versions_without_egress() {
    let (host, core, provider) = antigravity_fixture(1);
    assert!(matches!(
        block_on(core.quota_source(&provider, crate::CredentialId(7), 3, "subscription")),
        Err(CoreError::Unsupported)
    ));
    assert!(host.state.lock().unwrap().upstream_requests.is_empty());
}

#[test]
fn balance_probe_is_read_only_and_rejects_invalid_or_unauthorized_responses() {
    let host = MemoryHost::new(false);
    let (credential, version) = {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "deepseek".into();
        state.credential.secret["api_key"] = state.credential.secret["access_token"].clone();
        (state.credential.id, state.credential.version)
    };
    let channel: Box<dyn Channel> = Box::new(gproxy_channels::DeepSeekChannel);
    let core = Core::new(host.clone(), ChannelRegistry::new([channel]).unwrap()).unwrap();
    let mut provider = target().provider;
    provider.channel = "deepseek".into();
    provider.settings = json!({"base_url":"https://balance.example/v1"});
    {
        let mut state = host.state.lock().unwrap();
        state.scripted.extend([
            (StatusCode::OK, vec![Bytes::from_static(br#"{"is_available":true,"balance_infos":[{"currency":"CNY","total_balance":"110.00","granted_balance":"10.00","topped_up_balance":"100.00"}]}"#)]),
            (StatusCode::UNAUTHORIZED, vec![Bytes::from_static(b"denied")]),
            (StatusCode::OK, vec![Bytes::from_static(b"<html>not a balance endpoint</html>")]),
        ]);
    }
    let result = block_on(core.quota_source(&provider, credential, version, "balance")).unwrap();
    assert_eq!(result.entries.len(), 1);
    let QuotaValue::Balance(balance) = &result.entries[0].value else {
        panic!("balance")
    };
    assert_eq!(balance.remaining.unwrap(), 110.into());
    assert!(result.entries[0].observed_at_ms > 0);
    assert!(result.raw.is_empty());
    assert!(
        matches!(block_on(core.quota_source(&provider, credential, version, "balance")), Err(CoreError::UpstreamExhausted(message)) if message.contains("401"))
    );
    assert!(
        matches!(block_on(core.quota_source(&provider, credential, version, "balance")), Err(CoreError::UpstreamExhausted(message)) if message.contains("invalid data"))
    );
    assert!(matches!(
        block_on(core.quota_source(&provider, credential, version, "account_balance")),
        Err(CoreError::Unsupported)
    ));
    let state = host.state.lock().unwrap();
    assert_eq!(state.upstream_requests.len(), 3);
    assert_eq!(
        state.upstream_requests[0].1,
        "https://balance.example/user/balance"
    );
    assert!(state.health.is_empty());
    assert!(state.settlements.is_empty());
    assert_eq!(state.admit_calls, 0);
}

#[test]
fn management_report_paginates_and_keeps_inference_key_separate() {
    let host = MemoryHost::new(false);
    let (credential, version) = {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "openai".into();
        state.credential.secret = json!({"api_key":"inference-test","quota_api_key":"management-test","quota_channel":"openai"});
        (state.credential.id, state.credential.version)
    };
    let core = Core::new(
        host.clone(),
        ChannelRegistry::new([Box::new(gproxy_channels::OpenAiChannel) as Box<dyn Channel>])
            .unwrap(),
    )
    .unwrap();
    let mut provider = target().provider;
    provider.channel = "openai".into();
    provider.settings = json!({"base_url":"https://inference.example/v1"});
    let page = |start: i64, next: Option<&str>| {
        Bytes::from(json!({"has_more":next.is_some(),"next_page":next,"data":[{"start_time":start,"end_time":start+86400,"results":[{"amount":{"value":"1.2345","currency":"usd"}}]}]}).to_string())
    };
    host.state.lock().unwrap().scripted.extend([
        (StatusCode::OK, vec![page(86400, Some("next/+="))]),
        (StatusCode::OK, vec![page(172800, None)]),
    ]);
    let result =
        block_on(core.quota_source(&provider, credential, version, "organization_usage")).unwrap();
    assert_eq!(result.entries.len(), 2);
    let state = host.state.lock().unwrap();
    for (headers, uri) in &state.upstream_requests {
        assert_eq!(
            headers[http::header::AUTHORIZATION],
            "Bearer management-test"
        );
        assert!(uri.starts_with("https://api.openai.com/v1/organization/costs?"));
    }
    assert!(state.upstream_requests[1].1.contains("page=next%2F%2B%3D"));
    assert!(state.health.is_empty());
    drop(state);
    host.state.lock().unwrap().scripted.extend([
        (StatusCode::OK, vec![page(86400, Some("same"))]),
        (StatusCode::OK, vec![page(172800, Some("same"))]),
    ]);
    assert!(
        block_on(core.quota_source(&provider, credential, version, "organization_usage")).is_err()
    );
    host.state.lock().unwrap().credential.secret["quota_channel"] = json!("xai");
    let count = host.state.lock().unwrap().upstream_requests.len();
    assert!(matches!(
        block_on(core.quota_source(&provider, credential, version, "organization_usage")),
        Err(CoreError::Unsupported)
    ));
    assert_eq!(host.state.lock().unwrap().upstream_requests.len(), count);
}

#[test]
fn console_billing_uses_cookie_and_never_exposes_page_or_payment_data() {
    let host = MemoryHost::new(false);
    let (credential, version) = {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "opencode".into();
        state.credential.secret = json!({"api_key":"inference-fixture","quota_cookie":"auth=console-fixture; oc_locale=en","quota_workspace_id":"wrk_example","quota_channel":"opencode"});
        (state.credential.id, state.credential.version)
    };
    let core = Core::new(
        host.clone(),
        ChannelRegistry::new([Box::new(gproxy_channels::OpenCodeChannel) as Box<dyn Channel>])
            .unwrap(),
    )
    .unwrap();
    let mut provider = target().provider;
    provider.channel = "opencode".into();
    provider.settings = json!({"base_url":"https://inference.example/v1","tier":"zen"});
    let html=br#"<html><script>_$HY.r["billing.get[\"wrk_example\"]"]=$R[15]=$R[2]($R[16]={p:0,s:0,f:0});$R[22]($R[16],$R[23]={balance:0,monthlyLimit:null,monthlyUsage:null,timeMonthlyUsageUpdated:null,lite:$R[24]={}});</script></html>"#;
    host.state
        .lock()
        .unwrap()
        .scripted
        .push_back((StatusCode::OK, vec![Bytes::from_static(html)]));
    let result =
        block_on(core.quota_source(&provider, credential, version, "console_balance")).unwrap();
    assert_eq!(result.entries.len(), 2);
    let state = host.state.lock().unwrap();
    assert_eq!(
        state.upstream_requests[0].1,
        "https://opencode.ai/workspace/wrk_example/billing"
    );
    assert_eq!(
        state.upstream_requests[0].0[http::header::COOKIE],
        "auth=console-fixture"
    );
    assert!(
        !state.upstream_requests[0]
            .0
            .contains_key(http::header::AUTHORIZATION)
    );
    assert!(result.raw.is_empty());
    assert!(state.health.is_empty());
    assert!(state.settlements.is_empty());
}
