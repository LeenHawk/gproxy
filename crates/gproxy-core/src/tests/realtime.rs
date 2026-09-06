use bytes::Bytes;
use gproxy_channel_api::WsFrame;
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::json;

use super::memory::MemoryHost;
use super::{block_on, core, target};
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{FailoverBudget, Plan};
use crate::{CoreError, InitError};

mod resilience;
mod socket;

#[test]
fn call_is_rejected_before_egress_without_a_spawner() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    configure(&host);
    let core = core(&host)?;
    let error = block_on(core.execute(&host, request("request-no-spawner")))
        .expect_err("missing session spawner");
    assert!(matches!(error, CoreError::Unsupported));
    let state = host.state.lock().expect("state lock");
    assert!(state.loaded_credentials.is_empty());
    assert!(state.authorizations.is_empty());
    assert_eq!(state.socket_opens, 0);
    assert_eq!(state.admission_finishes, [false]);
    Ok(())
}

#[test]
fn call_defers_one_settlement_to_the_owned_sideband() -> Result<(), InitError> {
    let host = MemoryHost::with_session_spawner();
    configure(&host);
    let mut state = host.state.lock().expect("state lock");
    state.socket_frames = [
        WsFrame::Text(
            r#"{"type":"session.created","session":{"type":"realtime","model":"upstream-model-actual","audio":{"input":{"transcription":{"model":"transcription-model"}}}}}"#
                .into(),
        ),
        WsFrame::Text(
            r#"{"type":"response.done","response":{"usage":{"input_tokens":1000000,"output_tokens":1000000,"total_tokens":2000000}}}"#
                .into(),
        ),
        WsFrame::Text(
            r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"i1","transcript":"ok","usage":{"type":"tokens","input_tokens":1000000,"output_tokens":0,"total_tokens":1000000}}"#
                .into(),
        ),
        WsFrame::Close(Some(1000)),
    ]
    .into();
    drop(state);
    let core = core(&host)?;
    let outcome =
        block_on(core.execute(&host, request("request-realtime"))).expect("Realtime call");
    assert_eq!(outcome.status, StatusCode::OK);
    assert!(matches!(outcome.body, ResponseBody::Full(ref body) if body.as_ref() == b"v=answer"));

    let state = host.state.lock().expect("state lock");
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].cost, Decimal::from(6));
    assert_eq!(state.settlements[0].usage.input_tokens, 2_000_000);
    assert_eq!(state.settlements[0].upstream_model, "upstream-model-actual");
    assert_eq!(
        state.settlements[0].usage.dimensions["transcription_model"],
        "transcription-model"
    );
    assert_eq!(
        state.settlements[0].usage.metrics["session_model/primary/upstream-model-actual/cost"],
        Decimal::from(3)
    );
    assert_eq!(
        state.settlements[0].usage.metrics["session_model/transcription/transcription-model/cost"],
        Decimal::from(3)
    );
    assert_eq!(state.admission_finishes, [true]);
    assert_eq!(state.socket_opens, 1);
    assert_eq!(state.settlements[0].ended, crate::Ended::Complete);
    assert!(
        !state.settlements[0]
            .usage
            .metrics
            .contains_key("realtime_meter_compromised")
    );
    assert_eq!(state.captures.len(), 2);
    assert!(state.rotations.is_empty());
    assert!(state.cache.keys().all(|key| !key.contains("session-owner")));
    Ok(())
}

fn configure(host: &MemoryHost) {
    let mut state = host.state.lock().expect("state lock");
    state.credential.secret = json!({
        "access_token": "fresh",
        "expires_at": i64::MAX
    });
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
}

fn request(request_id: &str) -> RequestCtx {
    RequestCtx {
        request_id: request_id.into(),
        client_ip: None,
        method: Method::POST,
        path: "/v1/realtime/calls".into(),
        query: None,
        headers: HeaderMap::new(),
        body: Bytes::from_static(
            br#"{"sdp":"v=offer","session":{"type":"realtime","model":"alias","audio":{"input":{"transcription":{"model":"client-model"}}}}}"#,
        ),
        upgrade: false,
        mode: RoutingMode::Aggregated,
    }
}
