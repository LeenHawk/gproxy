use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream, UpstreamConnection,
    },
    connection::TransportError,
    connection::{HeaderMap, Method, StatusCode},
};

#[path = "support/capability_enforcement_host.rs"]
mod host;
use host::*;

#[test]
fn limits_cover_write_size_and_deadline_after_response_returns() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_millis(20),
        stream_idle: Duration::from_secs(1),
        read_bytes: 100,
        write_bytes: 3,
        ws_frame_bytes: 10,
    };
    let upstream = bounded(
        vec![
            (Duration::ZERO, Ok(Bytes::from_static(b"ok"))),
            (Duration::from_millis(30), Ok(Bytes::from_static(b"late"))),
        ],
        limits,
    );
    let target = ();
    let response = block_on(upstream.send(
        &target,
        request(HttpBody::Bytes(Bytes::from_static(b"abc"))),
    ))
    .unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("expected stream")
    };
    assert_eq!(
        block_on(stream.next()).unwrap().unwrap(),
        Bytes::from_static(b"ok")
    );
    let error = block_on(stream.next()).unwrap().unwrap_err();
    let capability = error.downcast_ref::<CapabilityError>().unwrap();
    assert_eq!(capability.kind(), CapabilityErrorKind::Limit);
    assert_eq!(capability.stage(), CapabilityErrorStage::Stream);
    assert!(block_on(stream.next()).is_none());
    let oversized = block_on(upstream.send(
        &target,
        request(HttpBody::Bytes(Bytes::from_static(b"toolong"))),
    ))
    .unwrap_err();
    assert_eq!(oversized.kind(), CapabilityErrorKind::Limit);
    assert_eq!(oversized.stage(), CapabilityErrorStage::BodyTransfer);
}

#[test]
fn idle_deadline_resets_on_each_progress_and_read_limit_is_enforced() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_secs(1),
        stream_idle: Duration::from_millis(30),
        read_bytes: 5,
        write_bytes: 20,
        ws_frame_bytes: 20,
    };
    let upstream = bounded(
        vec![
            (Duration::from_millis(20), Ok(Bytes::from_static(b"ab"))),
            (Duration::from_millis(20), Ok(Bytes::from_static(b"cd"))),
            (Duration::from_millis(5), Ok(Bytes::from_static(b"ef"))),
        ],
        limits,
    );
    let target = ();
    let response =
        block_on(upstream.send(&target, request(HttpBody::Bytes(Bytes::new())))).unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("expected stream")
    };
    assert_eq!(
        block_on(stream.next()).unwrap().unwrap(),
        Bytes::from_static(b"ab")
    );
    assert_eq!(
        block_on(stream.next()).unwrap().unwrap(),
        Bytes::from_static(b"cd")
    );
    let error = block_on(stream.next()).unwrap().unwrap_err();
    assert!(error.to_string().contains("read byte limit"));
    assert!(block_on(stream.next()).is_none());
}

#[test]
fn stream_error_retains_source_and_drop_stops_local_work() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_secs(1),
        stream_idle: Duration::from_secs(1),
        read_bytes: 100,
        write_bytes: 100,
        ws_frame_bytes: 100,
    };
    let upstream = bounded(
        vec![
            (Duration::ZERO, Ok(Bytes::from_static(b"first"))),
            (Duration::ZERO, Err(Box::new(MidError))),
        ],
        limits,
    );
    let target = ();
    let response =
        block_on(upstream.send(&target, request(HttpBody::Bytes(Bytes::new())))).unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("expected stream")
    };
    assert!(block_on(stream.next()).unwrap().is_ok());
    let source = block_on(stream.next()).unwrap().unwrap_err();
    let classified = source.downcast_ref::<CapabilityError>().unwrap();
    assert!(classified.source().unwrap().is::<MidError>());
    assert_eq!(
        classified.source().unwrap().to_string(),
        "source stream failed"
    );
    assert!(block_on(stream.next()).is_none());

    let dropped = Arc::new(Mutex::new(false));
    let body = HttpBody::Stream(Box::pin(DropStream {
        dropped: dropped.clone(),
    }));
    drop(body);
    assert!(*dropped.lock().unwrap());
}

#[test]
fn idle_timeout_rejects_empty_keepalives_and_ends_the_stream() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_secs(1),
        stream_idle: Duration::from_millis(10),
        read_bytes: 100,
        write_bytes: 100,
        ws_frame_bytes: 100,
    };
    let upstream = bounded(
        vec![
            (Duration::from_millis(6), Ok(Bytes::new())),
            (Duration::from_millis(5), Ok(Bytes::from_static(b"late"))),
        ],
        limits,
    );
    let response = block_on(upstream.send(&(), request(HttpBody::Bytes(Bytes::new())))).unwrap();
    let HttpBody::Stream(mut body) = response.body else {
        panic!("stream")
    };
    assert!(block_on(body.next()).unwrap().unwrap().is_empty());
    assert!(
        block_on(body.next())
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("idle deadline")
    );
    assert!(block_on(body.next()).is_none());
}

#[test]
fn total_deadline_includes_upload_and_time_before_first_response_poll() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_millis(20),
        stream_idle: Duration::from_secs(1),
        read_bytes: 100,
        write_bytes: 100,
        ws_frame_bytes: 100,
    };
    let upstream = bounded(
        vec![(Duration::from_millis(10), Ok(Bytes::from_static(b"late")))],
        limits,
    );
    let upload = TimedChunks {
        chunks: vec![(Duration::from_millis(15), Ok(Bytes::from_static(b"upload")))]
            .into_iter()
            .collect(),
        clock: upstream.clock.clone(),
    };
    let response =
        block_on(upstream.send(&(), request(HttpBody::Stream(Box::pin(upload))))).unwrap();
    let HttpBody::Stream(mut body) = response.body else {
        panic!("stream")
    };
    assert!(
        block_on(body.next())
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("operation deadline")
    );
    assert!(block_on(body.next()).is_none());

    let upstream = bounded(
        vec![(Duration::ZERO, Ok(Bytes::from_static(b"ready")))],
        limits,
    );
    let response = block_on(upstream.send(&(), request(HttpBody::Bytes(Bytes::new())))).unwrap();
    *upstream.clock.lock().unwrap() += Duration::from_millis(20);
    let HttpBody::Stream(mut body) = response.body else {
        panic!("stream")
    };
    assert!(
        block_on(body.next())
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("operation deadline")
    );
}

#[test]
fn streamed_upload_limits_and_pending_request_cancellation() {
    let limits = CapabilityLimits {
        operation_total: Duration::from_secs(1),
        stream_idle: Duration::from_secs(1),
        read_bytes: 100,
        write_bytes: 3,
        ws_frame_bytes: 100,
    };
    let upstream = bounded(vec![], limits);
    let upload = TimedChunks {
        chunks: vec![
            (Duration::ZERO, Ok(Bytes::from_static(b"ab"))),
            (Duration::ZERO, Ok(Bytes::from_static(b"cd"))),
        ]
        .into_iter()
        .collect(),
        clock: upstream.clock.clone(),
    };
    let error =
        block_on(upstream.send(&(), request(HttpBody::Stream(Box::pin(upload))))).unwrap_err();
    assert_eq!(error.kind(), CapabilityErrorKind::Limit);
    assert_eq!(error.stage(), CapabilityErrorStage::BodyTransfer);
    let dropped = Arc::new(Mutex::new(false));
    let upload = HttpBody::Stream(Box::pin(DropStream {
        dropped: dropped.clone(),
    }));
    let mut future = upstream.send(&(), request(upload));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert!(!*dropped.lock().unwrap());
    drop(future);
    assert!(*dropped.lock().unwrap());
}
