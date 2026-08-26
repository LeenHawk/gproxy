use futures_util::StreamExt as _;
use http::{Method, StatusCode};
use serde_json::json;

use super::super::memory::MemoryHost;
use super::super::surface_harness::{outcome, target};
use super::super::{block_on, core as build_core};
use crate::control::{FailoverBudget, Plan};
use crate::{CoreError, CredentialId, ResponseBody};

#[test]
fn declaration_controls_budgeted_forward_failover() {
    let host = make_host(2, [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK]);
    let core = build_core(&host).expect("core");
    let success = outcome(
        &core,
        &host,
        Method::GET,
        "/surface/retry",
        Some(("x-retry-session", "session-1")),
        None,
        false,
    )
    .expect("retryable forward");
    assert_eq!(success.status, StatusCode::OK);
    drain(success.body);
    {
        let state = host.state.lock().expect("state lock");
        assert_eq!(state.loaded_credentials, [CredentialId(7), CredentialId(8)]);
        assert_eq!(state.captures.len(), 2);
        assert_eq!(state.admit_calls, 1);
        assert_eq!(state.admission_finishes, [true]);
        assert!(
            state
                .cache
                .values()
                .any(|value| value.as_slice() == 8_i64.to_be_bytes())
        );
    }

    let host = make_host(1, [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK]);
    let core = build_core(&host).expect("core");
    let exhausted = outcome(
        &core,
        &host,
        Method::GET,
        "/surface/retry",
        None,
        None,
        false,
    );
    assert!(matches!(exhausted, Err(CoreError::UpstreamExhausted(_))));
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.loaded_credentials, [CredentialId(7)]);
    assert_eq!(state.captures.len(), 1);
    assert_eq!(state.admission_finishes, [false]);
    drop(state);

    let host = make_host(2, [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK]);
    let core = build_core(&host).expect("core");
    let mutation = outcome(
        &core,
        &host,
        Method::POST,
        "/surface/mutate",
        None,
        Some(b"{}"),
        false,
    )
    .expect("single-attempt mutation");
    assert_eq!(mutation.status, StatusCode::TOO_MANY_REQUESTS);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.loaded_credentials, [CredentialId(7)]);
    assert_eq!(state.captures.len(), 1);
    assert_eq!(state.admission_finishes, [true]);
}

fn make_host<const N: usize>(budget: u32, statuses: [StatusCode; N]) -> MemoryHost {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state.credential.secret = json!({
        "access_token": "fresh",
        "expires_at": i64::MAX
    });
    state.plan = Some(Plan {
        targets: vec![target(3, 7), target(3, 8)],
        budget: FailoverBudget {
            max_attempts: budget,
        },
    });
    state.statuses = statuses.into();
    drop(state);
    host
}

fn drain(body: ResponseBody) {
    let ResponseBody::Stream(mut stream) = body else {
        panic!("successful read forward was not streamed");
    };
    block_on(async {
        while let Some(chunk) = stream.next().await {
            chunk.expect("forward stream");
        }
    });
}
