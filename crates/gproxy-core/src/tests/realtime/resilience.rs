use gproxy_channel_api::WsFrame;
use http::StatusCode;
use rust_decimal::Decimal;

use super::super::memory::MemoryHost;
use super::super::{block_on, core};
use super::{configure, request};
use crate::InitError;

#[test]
fn duplicate_call_id_cannot_open_a_second_sideband() -> Result<(), InitError> {
    let host = MemoryHost::with_continuations();
    configure(&host);
    host.state.lock().expect("state lock").socket_frames = [WsFrame::Text(
        r#"{"type":"session.created","session":{"type":"realtime","model":"upstream-model"}}"#
            .into(),
    )]
    .into();
    let core = core(&host)?;
    let first = block_on(core.execute(&host, request("request-owner"))).expect("first call");
    assert_eq!(first.status, StatusCode::OK);
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .cache_ttls
            .values()
            .any(|ttl| *ttl == 300)
    );
    let duplicate =
        block_on(core.execute(&host, request("request-duplicate"))).expect("error outcome");
    assert_eq!(duplicate.status, StatusCode::INTERNAL_SERVER_ERROR);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.socket_opens, 1);
    assert_eq!(state.admission_finishes, [false]);
    Ok(())
}

#[test]
fn cancelled_meter_releases_owner_and_settles_interrupted() -> Result<(), InitError> {
    let host = MemoryHost::with_cancelling_session_spawner();
    configure(&host);
    host.state.lock().expect("state lock").socket_frames = [WsFrame::Text(
        r#"{"type":"session.created","session":{"type":"realtime","model":"upstream-model"}}"#
            .into(),
    )]
    .into();
    let core = core(&host)?;
    let outcome = block_on(core.execute(&host, request("request-cancelled"))).expect("call");
    assert_eq!(outcome.status, StatusCode::OK);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].ended, crate::Ended::Interrupted);
    assert_eq!(state.admission_finishes, [true]);
    assert!(state.cache.keys().all(|key| !key.contains("session-owner")));
    Ok(())
}

#[test]
fn gone_call_stops_reconnect_and_finally_settles() -> Result<(), InitError> {
    let host = MemoryHost::with_session_spawner();
    configure(&host);
    let mut state = host.state.lock().expect("state lock");
    state.socket_frames = [
        WsFrame::Text(
            r#"{"type":"session.created","session":{"type":"realtime","model":"upstream-model"}}"#
                .into(),
        ),
        WsFrame::Text(
            r#"{"type":"response.done","response":{"id":"r1","usage":{"input_tokens":1000000,"output_tokens":0,"total_tokens":1000000}}}"#
                .into(),
        ),
        WsFrame::Close(Some(1011)),
    ]
    .into();
    state.socket_statuses = [101, 410].into();
    drop(state);
    let core = core(&host)?;
    let outcome = block_on(core.execute(&host, request("request-gone"))).expect("call");
    assert_eq!(outcome.status, StatusCode::OK);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.socket_opens, 2);
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].cost, Decimal::ONE);
    assert_eq!(state.settlements[0].ended, crate::Ended::Interrupted);
    assert_eq!(state.admission_finishes, [true]);
    assert!(state.cache.keys().all(|key| !key.contains("session-owner")));
    Ok(())
}
