use super::memory::MemoryHost;
use super::{block_on, core, request, target};
use crate::control::{FailoverBudget, Plan};

#[test]
fn public_funnel_applies_the_channel_response_header_allowlist() -> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = core(&host)?;
    let outcome = block_on(core.execute(&host, request(false, "response-header-allowlist")))
        .expect("request succeeds");
    assert_eq!(outcome.headers["x-test-visible"], "kept");
    assert!(!outcome.headers.contains_key("x-test-hidden"));
    assert!(!outcome.headers.contains_key("set-cookie"));
    Ok(())
}

#[test]
fn provider_policy_overrides_channel_defaults_but_not_global_denials()
-> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    let mut selected = target();
    selected.provider.traffic_blacklist = gproxy_channel_api::TrafficBlacklistConfig::new(
        vec!["x-provider-blocked".into()],
        vec!["x-test-hidden".into()],
        vec!["page".into()],
    )
    .unwrap();
    selected.provider.settings = serde_json::json!({
        "traffic_policy": {
            "request_headers": ["x-provider-*", "authorization"],
            "response_headers": ["x-test-hidden", "x-test-visible", "set-cookie"],
            "request_query": ["cursor", "page", "key"]
        }
    });
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![selected],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = core(&host)?;
    let mut request = request(false, "provider-traffic-policy");
    request.query = Some("cursor=3&page=2&key=secret&ignored=yes".into());
    request
        .headers
        .insert("x-provider-trace", "kept".parse().unwrap());
    request
        .headers
        .insert("x-other", "dropped".parse().unwrap());
    request
        .headers
        .insert("x-provider-blocked", "dropped".parse().unwrap());
    let outcome = block_on(core.execute(&host, request)).expect("request succeeds");
    let state = host.state.lock().expect("state lock");
    let (headers, uri) = state.upstream_requests.last().expect("upstream request");
    assert_eq!(headers["x-provider-trace"], "kept");
    assert!(!headers.contains_key("x-provider-blocked"));
    assert!(!headers.contains_key("x-other"));
    assert!(uri.ends_with("?cursor=3"));
    assert!(!outcome.headers.contains_key("x-test-hidden"));
    assert_eq!(outcome.headers["x-test-visible"], "kept");
    assert!(!outcome.headers.contains_key("set-cookie"));
    Ok(())
}
