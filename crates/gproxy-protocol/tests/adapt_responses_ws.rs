use futures_core::Stream;
use futures_sink::Sink;
use gproxy_protocol::{
    HttpBody, WebSocket, WireRequest, WireResponse,
    adapt::responses_ws::{
        self, ResponsesWsConnect, ResponsesWsLimits, ResponsesWsSession, ResponsesWsTurn,
    },
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, Upstream, UpstreamConnection,
    },
    connection::{Bytes, HeaderMap, StatusCode, TransportError, WsClose, WsFrame},
    transform::{
        TransformError, TransformErrorKind,
        generate::stream::responses::{ResponsesStreamCollector, synthesize_responses_stream},
        identity::{IdNamespace, IdentityFlow},
    },
    wire::openai::responses::{
        generate::GenerateContentRequestBody, response as r, stream::StreamEvent,
    },
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    error::Error,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};

struct Shared {
    incoming: VecDeque<Result<WsFrame, TransportError>>,
    batches: VecDeque<Vec<Result<WsFrame, TransportError>>>,
    sent: Vec<WsFrame>,
    ready: bool,
    flush: bool,
    eof: bool,
    fail_ready: bool,
    fail_flush: bool,
    fail_start: bool,
    ready_polls: usize,
    flush_polls: usize,
    receive_polls: usize,
    sender_drops: usize,
    receiver_drops: usize,
    waker: Option<Waker>,
}
impl Shared {
    fn new(batches: Vec<Vec<Result<WsFrame, TransportError>>>) -> Self {
        Self {
            incoming: VecDeque::new(),
            batches: batches.into(),
            sent: Vec::new(),
            ready: true,
            flush: true,
            eof: false,
            fail_ready: false,
            fail_flush: false,
            fail_start: false,
            ready_polls: 0,
            flush_polls: 0,
            receive_polls: 0,
            sender_drops: 0,
            receiver_drops: 0,
            waker: None,
        }
    }
}
struct Incoming(Arc<Mutex<Shared>>);
impl Stream for Incoming {
    type Item = Result<WsFrame, TransportError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut s = self.0.lock().unwrap();
        s.receive_polls += 1;
        if let Some(v) = s.incoming.pop_front() {
            Poll::Ready(Some(v))
        } else if s.eof {
            Poll::Ready(None)
        } else {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
impl Drop for Incoming {
    fn drop(&mut self) {
        self.0.lock().unwrap().receiver_drops += 1;
    }
}
struct Outgoing(Arc<Mutex<Shared>>);
fn transport() -> TransportError {
    Box::new(std::io::Error::other("transport fixture failure"))
}
impl Sink<WsFrame> for Outgoing {
    type Error = TransportError;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut s = self.0.lock().unwrap();
        s.ready_polls += 1;
        if s.fail_ready {
            Poll::Ready(Err(transport()))
        } else if s.ready {
            Poll::Ready(Ok(()))
        } else {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
    fn start_send(self: Pin<&mut Self>, frame: WsFrame) -> Result<(), Self::Error> {
        let mut s = self.0.lock().unwrap();
        let request = matches!(frame, WsFrame::Text(_));
        s.sent.push(frame);
        if s.fail_start {
            return Err(transport());
        }
        if request && let Some(batch) = s.batches.pop_front() {
            s.incoming.extend(batch);
        }
        Ok(())
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut s = self.0.lock().unwrap();
        s.flush_polls += 1;
        if s.fail_flush {
            Poll::Ready(Err(transport()))
        } else if s.flush {
            Poll::Ready(Ok(()))
        } else {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
    fn poll_close(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}
impl Drop for Outgoing {
    fn drop(&mut self) {
        self.0.lock().unwrap().sender_drops += 1;
    }
}
struct Unread(Arc<AtomicUsize>);
impl Stream for Unread {
    type Item = Result<Bytes, TransportError>;
    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(Some(Ok(Bytes::from_static(b"native raw rejection"))))
    }
}
struct Host {
    shared: Arc<Mutex<Shared>>,
    caps: CapabilityLimits,
    connections: AtomicUsize,
    requests: Mutex<Vec<(String, WireRequest<()>)>>,
    status: StatusCode,
    rejected: bool,
    fail_connect: bool,
    rejection_polls: Arc<AtomicUsize>,
}
impl Host {
    fn new(batches: Vec<Vec<Result<WsFrame, TransportError>>>) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared::new(batches))),
            caps: caps(),
            connections: AtomicUsize::new(0),
            requests: Mutex::default(),
            status: StatusCode::SWITCHING_PROTOCOLS,
            rejected: false,
            fail_connect: false,
            rejection_polls: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl Upstream for Host {
    type Target = String;
    fn send<'a>(
        &'a self,
        _: &'a String,
        _: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        panic!("native WS must never fall back to HTTP send")
    }
    fn connect<'a>(
        &'a self,
        target: &'a String,
        request: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        self.connections.fetch_add(1, Ordering::SeqCst);
        self.requests
            .lock()
            .unwrap()
            .push((target.clone(), request));
        Box::pin(async move {
            if self.fail_connect {
                return Err(CapabilityError::new(
                    CapabilityErrorKind::Transport,
                    CapabilityErrorStage::Start,
                    "connect failed",
                ));
            }
            let mut headers = HeaderMap::new();
            headers.append("x-handshake", "one".parse().unwrap());
            headers.append("x-handshake", "two".parse().unwrap());
            if self.rejected {
                return Ok(UpstreamConnection::Rejected(WireResponse {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    headers,
                    body: HttpBody::Stream(Box::pin(Unread(self.rejection_polls.clone()))),
                }));
            }
            Ok(UpstreamConnection::Connected {
                handshake: WireResponse {
                    status: self.status,
                    headers,
                    body: (),
                },
                socket: WebSocket {
                    incoming: Box::pin(Incoming(self.shared.clone())),
                    outgoing: Box::pin(Outgoing(self.shared.clone())),
                },
            })
        })
    }
    fn limits(&self) -> CapabilityLimits {
        self.caps
    }
}
fn caps() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: Duration::from_secs(30),
        stream_idle: Duration::from_secs(5),
        read_bytes: 65536,
        write_bytes: 65536,
        ws_frame_bytes: 65536,
    }
}
fn handshake() -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path: "/v1/responses".into(),
        query: Some("a=1&a=2".into()),
        headers: HeaderMap::new(),
        body: (),
    }
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture should complete immediately"),
    }
}
fn open(host: &Host, limits: ResponsesWsLimits) -> ResponsesWsSession {
    let ResponsesWsConnect::Connected { handshake, session } = ready(responses_ws::connect(
        host,
        &"selected-account".into(),
        handshake(),
        limits,
    ))
    .unwrap() else {
        panic!("expected connected")
    };
    assert_eq!(handshake.status, host.status);
    assert_eq!(handshake.headers.get_all("x-handshake").iter().count(), 2);
    session
}
fn request() -> GenerateContentRequestBody {
    serde_json::from_value(json!({"model":"m","input":"hello","max_output_tokens":16})).unwrap()
}
fn response(id: &str) -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":id,"created_at":123,"error":null,"incomplete_details":null,"instructions":null,"metadata":null,"model":"m","object":"response","output":[{"type":"message","id":format!("msg-{id}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"ok","annotations":[],"logprobs":[]}]}],"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"status":"completed","usage":{"input_tokens":5,"output_tokens":7,"total_tokens":12,"input_tokens_details":{"cached_tokens":1,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":2}}})).unwrap()
}
fn events(body: r::GenerateContentResponseBody) -> Vec<StreamEvent> {
    synthesize_responses_stream(
        body,
        &mut IdentityFlow::new(IdNamespace::with_bytes([7; 16])),
        Default::default(),
    )
    .unwrap()
    .value
}
fn frames_for(body: r::GenerateContentResponseBody) -> Vec<Result<WsFrame, TransportError>> {
    events(body)
        .into_iter()
        .map(|event| Ok(WsFrame::Text(serde_json::to_string(&event).unwrap())))
        .collect()
}
fn frames(id: &str) -> Vec<Result<WsFrame, TransportError>> {
    frames_for(response(id))
}
fn frame(value: Value) -> Result<WsFrame, TransportError> {
    Ok(WsFrame::Text(serde_json::to_string(&value).unwrap()))
}
fn poll(turn: &mut ResponsesWsTurn<'_>) -> Poll<Option<Result<StreamEvent, TransformError>>> {
    Pin::new(turn).poll_next(&mut Context::from_waker(Waker::noop()))
}
fn drain(turn: &mut ResponsesWsTurn<'_>) -> Result<Vec<StreamEvent>, TransformError> {
    let mut out = Vec::new();
    for _ in 0..200_000 {
        match poll(turn) {
            Poll::Ready(Some(Ok(event))) => out.push(event),
            Poll::Ready(Some(Err(e))) => return Err(e),
            Poll::Ready(None) => return Ok(out),
            Poll::Pending => {}
        }
    }
    panic!("stream did not terminate within fixture poll budget")
}
fn text_sends(shared: &Arc<Mutex<Shared>>) -> usize {
    shared
        .lock()
        .unwrap()
        .sent
        .iter()
        .filter(|v| matches!(v, WsFrame::Text(_)))
        .count()
}
fn assert_closed(host: &Host) {
    let s = host.shared.lock().unwrap();
    assert_eq!(s.sender_drops, 1);
    assert_eq!(s.receiver_drops, 1);
}
fn collected(events: Vec<StreamEvent>) -> r::GenerateContentResponseBody {
    let mut c = ResponsesStreamCollector::new(Default::default());
    for e in events {
        c.push(e).unwrap();
    }
    c.finish().unwrap().value
}

#[test]
fn native_events_are_incremental_and_two_turns_forward_exact_native_continuation() {
    let host = Host::new(vec![frames("resp-native-1"), frames("resp-native-2")]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    let Poll::Ready(Some(Ok(first))) = poll(&mut turn) else {
        panic!()
    };
    assert!(matches!(first, StreamEvent::Created(_)));
    assert_eq!(text_sends(&host.shared), 1);
    let mut output = vec![first];
    output.extend(drain(&mut turn).unwrap());
    drop(turn);
    assert_eq!(collected(output), response("resp-native-1"));
    assert_eq!(session.last_response_id(), Some("resp-native-1"));
    assert_eq!(session.last_status(), Some(r::ResponseStatus::Completed));
    assert!(!session.is_busy());
    let mut next = request();
    next.previous_response_id = Some(Some("resp-native-1".into()));
    let mut turn = session.turn(next).unwrap();
    let output = drain(&mut turn).unwrap();
    drop(turn);
    assert_eq!(collected(output), response("resp-native-2"));
    assert_eq!(text_sends(&host.shared), 2);
    assert_eq!(host.connections.load(Ordering::SeqCst), 1);
    let sent = host.shared.lock().unwrap();
    let WsFrame::Text(text) = &sent.sent[1] else {
        panic!()
    };
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["type"], "response.create");
    assert_eq!(v["previous_response_id"], "resp-native-1");
    assert_eq!(v["max_output_tokens"], 16);
    let requests = host.requests.lock().unwrap();
    assert_eq!(requests[0].0, "selected-account");
    assert_eq!(requests[0].1.path, "/v1/responses");
    assert_eq!(requests[0].1.query.as_deref(), Some("a=1&a=2"));
}
#[test]
fn pending_pong_readiness_and_flush_never_reconstruct_a_generation() {
    let mut batch = vec![Ok(WsFrame::Ping(Bytes::from_static(b"ping-payload")))];
    batch.extend(frames("r"));
    let host = Host::new(vec![batch]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    assert!(poll(&mut turn).is_pending());
    assert_eq!(text_sends(&host.shared), 1);
    {
        let mut s = host.shared.lock().unwrap();
        s.ready = false;
        s.flush = false;
    }
    assert!(poll(&mut turn).is_pending());
    assert!(poll(&mut turn).is_pending());
    {
        let s = host.shared.lock().unwrap();
        assert_eq!(s.sent.len(), 1);
        assert_eq!(s.receive_polls, 1);
    }
    host.shared.lock().unwrap().ready = true;
    assert!(poll(&mut turn).is_pending());
    let ready_polls = host.shared.lock().unwrap().ready_polls;
    assert!(poll(&mut turn).is_pending());
    {
        let s = host.shared.lock().unwrap();
        assert_eq!(s.ready_polls, ready_polls);
        assert_eq!(s.receive_polls, 1);
        assert_eq!(s.sent.len(), 2);
        assert!(matches!(&s.sent[1],WsFrame::Pong(p)if p.as_ref()==b"ping-payload"));
    }
    host.shared.lock().unwrap().flush = true;
    let output = drain(&mut turn).unwrap();
    assert_eq!(collected(output), response("r"));
    drop(turn);
    assert_eq!(text_sends(&host.shared), 1);
    assert!(!session.is_poisoned());
}
#[test]
fn pending_request_readiness_and_flush_send_the_exact_frame_once() {
    let host = Host::new(vec![frames("r")]);
    {
        let mut s = host.shared.lock().unwrap();
        s.ready = false;
        s.flush = false;
    }
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    assert!(poll(&mut turn).is_pending());
    assert_eq!(text_sends(&host.shared), 0);
    assert_eq!(host.shared.lock().unwrap().receive_polls, 0);
    host.shared.lock().unwrap().ready = true;
    assert!(poll(&mut turn).is_pending());
    assert!(poll(&mut turn).is_pending());
    assert_eq!(text_sends(&host.shared), 1);
    assert_eq!(host.shared.lock().unwrap().receive_polls, 0);
    host.shared.lock().unwrap().flush = true;
    assert_eq!(collected(drain(&mut turn).unwrap()), response("r"));
}
#[test]
fn cancellation_before_send_during_ready_flush_receive_and_pong_closes_handles() {
    for stage in 0..5 {
        let batch = if stage == 4 {
            vec![Ok(WsFrame::Ping(Bytes::from_static(b"p")))]
        } else {
            vec![]
        };
        let host = Host::new(vec![batch]);
        if stage == 1 {
            host.shared.lock().unwrap().ready = false;
        }
        if stage == 2 {
            host.shared.lock().unwrap().flush = false;
        }
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        if stage != 0 {
            assert!(poll(&mut turn).is_pending());
        }
        drop(turn);
        assert!(session.is_poisoned());
        assert!(session.turn(request()).is_err());
        assert_closed(&host);
        assert_eq!(text_sends(&host.shared), usize::from(stage >= 2));
    }
}
#[test]
fn leaked_uncertain_turn_cannot_reuse_session_but_leaked_terminal_turn_can() {
    for phase in 0..3 {
        let host = Host::new(vec![]);
        if phase == 2 {
            host.shared.lock().unwrap().flush = false;
        }
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        if phase > 0 {
            assert!(poll(&mut turn).is_pending());
        }
        std::mem::forget(turn);
        assert!(session.is_busy());
        assert!(session.turn(request()).is_err());
        assert_eq!(text_sends(&host.shared), usize::from(phase > 0));
    }
    let host = Host::new(vec![frames("a"), frames("b")]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    loop {
        if let Poll::Ready(Some(Ok(StreamEvent::Completed(_)))) = poll(&mut turn) {
            break;
        }
    }
    std::mem::forget(turn);
    assert!(!session.is_busy());
    assert_eq!(session.last_response_id(), Some("a"));
    let mut turn = session.turn(request()).unwrap();
    assert_eq!(collected(drain(&mut turn).unwrap()), response("b"));
}
#[test]
fn host_read_and_write_caps_are_directional_and_preflight_failure_is_reusable() {
    let mut host = Host::new(vec![frames("r")]);
    host.caps.write_bytes = 128;
    let mut session = open(&host, Default::default());
    let mut too_large = request();
    too_large.input =
        Some(gproxy_protocol::wire::openai::responses::input::Input::Text("x".repeat(200)));
    assert!(session.turn(too_large).is_err());
    assert!(!session.is_busy());
    assert!(!session.is_poisoned());
    assert_eq!(text_sends(&host.shared), 0);
    let mut turn = session.turn(request()).unwrap();
    assert_eq!(collected(drain(&mut turn).unwrap()), response("r"));
}
#[test]
fn impossible_host_limits_fail_before_connect() {
    for field in 0..2 {
        let mut host = Host::new(vec![]);
        let req = handshake();
        match field {
            0 => host.caps.write_bytes = 1,
            _ => host.caps.ws_frame_bytes = 0,
        };
        assert!(
            ready(responses_ws::connect(
                &host,
                &"t".into(),
                req,
                Default::default()
            ))
            .is_err()
        );
        assert_eq!(host.connections.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn raw_handshake_rejection_is_unread_and_connected_status_is_not_fabricated() {
    let mut host = Host::new(vec![]);
    host.rejected = true;
    let ResponsesWsConnect::Rejected(r) = ready(responses_ws::connect(
        &host,
        &"t".into(),
        handshake(),
        Default::default(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.headers.get_all("x-handshake").iter().count(), 2);
    assert!(matches!(r.body, HttpBody::Stream(_)));
    assert_eq!(host.rejection_polls.load(Ordering::SeqCst), 0);
    let mut host = Host::new(vec![]);
    host.status = StatusCode::FORBIDDEN;
    let error = ready(responses_ws::connect(
        &host,
        &"t".into(),
        handshake(),
        Default::default(),
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
    assert_eq!(error.handshake.unwrap().status, StatusCode::FORBIDDEN);
    assert_closed(&host);
    let mut host = Host::new(vec![]);
    host.status = StatusCode::OK;
    let session = open(&host, Default::default());
    assert!(!session.is_poisoned());
}
#[test]
fn failure_and_error_events_preserve_native_payloads_and_poison() {
    for failed in [false, true] {
        let batch = if failed {
            let mut start = serde_json::to_value(response("r")).unwrap();
            start["status"] = json!("in_progress");
            start["output"] = json!([]);
            start["usage"] = Value::Null;
            let mut fail = start.clone();
            fail["status"] = json!("failed");
            fail["error"] = json!({"code":"rate_limit_exceeded","message":"native quota detail"});
            vec![
                frame(json!({"type":"response.created","sequence_number":0,"response":start})),
                frame(json!({"type":"response.failed","sequence_number":1,"response":fail})),
            ]
        } else {
            vec![frame(
                json!({"type":"error","sequence_number":0,"code":"previous_response_not_found","message":"native missing ID","param":"previous_response_id"}),
            )]
        };
        let host = Host::new(vec![batch]);
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        let error = drain(&mut turn).unwrap_err();
        assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
        let fact = serde_json::to_value(turn.failure_event().unwrap()).unwrap();
        if failed {
            assert_eq!(fact["response"]["error"]["message"], "native quota detail");
        } else {
            assert_eq!(fact["param"], "previous_response_id");
            assert_eq!(fact["code"], "previous_response_not_found");
        }
        drop(turn);
        assert!(session.is_poisoned());
        assert!(session.last_response_id().is_none());
        assert!(session.turn(request()).is_err());
        assert_closed(&host);
    }
}
#[test]
fn malformed_source_lifecycles_never_expose_terminal_success() {
    for issue in 0..5 {
        let mut es = events(response("r"));
        match issue {
            0 => {
                es.remove(0);
            }
            1 => {
                let last = es.last_mut().unwrap();
                let StreamEvent::Completed(e) = last else {
                    panic!()
                };
                e.response.output.clear();
            }
            2 => {
                let duplicate = es[0].clone();
                es.insert(1, duplicate);
            }
            3 => {
                es.pop();
            }
            _ => {
                let last = es.last_mut().unwrap();
                let StreamEvent::Completed(e) = last else {
                    panic!()
                };
                e.response.status = Some(r::ResponseStatus::InProgress);
            }
        }
        let batch = es
            .into_iter()
            .map(|e| Ok(WsFrame::Text(serde_json::to_string(&e).unwrap())))
            .collect();
        let host = Host::new(vec![batch]);
        host.shared.lock().unwrap().eof = true;
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        assert!(drain(&mut turn).is_err());
        drop(turn);
        assert!(session.is_poisoned());
        assert!(session.last_status().is_none());
        assert_closed(&host);
    }
}
#[test]
fn genuine_incomplete_is_preserved_and_connection_is_reusable() {
    let mut v = serde_json::to_value(response("partial")).unwrap();
    v["status"] = json!("incomplete");
    v["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let partial: r::GenerateContentResponseBody = serde_json::from_value(v).unwrap();
    let host = Host::new(vec![frames_for(partial.clone()), frames("next")]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    let output = drain(&mut turn).unwrap();
    assert!(matches!(output.last(), Some(StreamEvent::Incomplete(_))));
    assert_eq!(collected(output), partial);
    drop(turn);
    assert_eq!(session.last_status(), Some(r::ResponseStatus::Incomplete));
    let mut turn = session.turn(request()).unwrap();
    assert_eq!(collected(drain(&mut turn).unwrap()), response("next"));
}
#[derive(Default)]
struct Wakes(AtomicUsize);
impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn empty_control_frames_yield_cooperatively_without_recursive_polling() {
    let host = Host::new(vec![
        (0..10_000)
            .map(|_| Ok(WsFrame::Pong(Bytes::new())))
            .collect(),
    ]);
    let mut session = open(&host, ResponsesWsLimits::default());
    let mut turn = session.turn(request()).unwrap();
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(wakes.clone());
    for _ in 0..8 {
        assert!(
            Pin::new(&mut turn)
                .poll_next(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    assert_eq!(wakes.0.load(Ordering::SeqCst), 8);
    assert_eq!(host.shared.lock().unwrap().receive_polls, 8);
    assert_eq!(turn.received_frames(), 8);
    drop(turn);
    assert_closed(&host);
}
#[test]
fn control_and_data_bytes_share_receive_budget_and_pongs_share_send_budget() {
    let host = Host::new(vec![vec![
        Ok(WsFrame::Ping(Bytes::from_static(b"ab"))),
        Ok(WsFrame::Pong(Bytes::from_static(b"cd"))),
    ]]);
    let mut session = open(
        &host,
        ResponsesWsLimits {
            max_receive_bytes: 3,
            ..Default::default()
        },
    );
    let mut turn = session.turn(request()).unwrap();
    assert_eq!(
        drain(&mut turn).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    drop(turn);
    assert_closed(&host);
    let encoded =
        serde_json::to_vec(
            &gproxy_protocol::wire::openai::responses::websocket::ClientEvent::ResponseCreate(
                request(),
            ),
        )
        .unwrap()
        .len();
    let host = Host::new(vec![vec![Ok(WsFrame::Ping(Bytes::from_static(b"p")))]]);
    let mut session = open(
        &host,
        ResponsesWsLimits {
            max_send_bytes: encoded,
            ..Default::default()
        },
    );
    let mut turn = session.turn(request()).unwrap();
    assert_eq!(
        drain(&mut turn).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    assert_eq!(host.shared.lock().unwrap().sent.len(), 1);
}
#[test]
fn binary_policy_close_facts_and_transport_errors_are_explicit() {
    for allow in [false, true] {
        let binary = frames("r")
            .into_iter()
            .map(|f| {
                let Ok(WsFrame::Text(text)) = f else { panic!() };
                Ok(WsFrame::Binary(Bytes::from(text)))
            })
            .collect();
        let host = Host::new(vec![binary]);
        let mut session = open(
            &host,
            ResponsesWsLimits {
                allow_binary: allow,
                ..Default::default()
            },
        );
        let mut turn = session.turn(request()).unwrap();
        let result = drain(&mut turn);
        assert_eq!(result.is_ok(), allow);
        if !allow {
            assert_eq!(result.unwrap_err().kind(), TransformErrorKind::Unsupported);
        }
    }
    let host = Host::new(vec![vec![Ok(WsFrame::Close(Some(WsClose {
        code: 1008,
        reason: "native policy".into(),
    })))]]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    assert!(drain(&mut turn).is_err());
    drop(turn);
    assert_eq!(session.peer_close().unwrap().reason, "native policy");
    assert_eq!(session.peer_close().unwrap().code, 1008);
    for point in 0..4 {
        let host = Host::new(vec![vec![]]);
        {
            let mut s = host.shared.lock().unwrap();
            match point {
                0 => s.fail_ready = true,
                1 => s.fail_start = true,
                2 => s.fail_flush = true,
                _ => s.batches = vec![vec![Err(transport())]].into(),
            }
        }
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        let e = drain(&mut turn).unwrap_err();
        assert_eq!(e.kind(), TransformErrorKind::Host);
        assert!(e.source().is_some());
        drop(turn);
        assert_closed(&host);
        assert!(session.turn(request()).is_err());
    }
}
#[test]
fn declared_request_and_event_fields_survive_without_foreign_rest() {
    let mut batch = frames("r");
    let Ok(WsFrame::Text(text)) = &batch[0] else {
        panic!()
    };
    let mut v: Value = serde_json::from_str(text).unwrap();
    v["x-foreign"] = json!({"poison":true});
    v["response"]["x-foreign"] = json!("DROP");
    batch[0] = frame(v);
    let host = Host::new(vec![batch]);
    let mut session = open(&host, Default::default());
    let req=serde_json::from_value(json!({"model":"m","input":"hello","previous_response_id":"native-id","tools":[{"type":"function","name":"f","parameters":{"type":"object","x-formal":true},"strict":true}],"x-foreign":"DROP"})).unwrap();
    let mut turn = session.turn(req).unwrap();
    let output = drain(&mut turn).unwrap();
    assert!(
        !serde_json::to_string(&output)
            .unwrap()
            .contains("x-foreign")
    );
    let s = host.shared.lock().unwrap();
    let WsFrame::Text(text) = &s.sent[0] else {
        panic!()
    };
    let v: Value = serde_json::from_str(text).unwrap();
    assert_eq!(v["previous_response_id"], "native-id");
    assert_eq!(v["tools"][0]["parameters"]["x-formal"], true);
    assert!(v.get("x-foreign").is_none());
}

#[test]
fn documented_nullable_created_and_in_progress_frames_work_over_websocket() {
    let mut values: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/responses_nullable_frames.json")).unwrap();
    let mut terminal = values[0]["response"].clone();
    terminal["status"] = json!("completed");
    values.push(json!({"type":"response.completed","sequence_number":2,"response":terminal}));
    let host = Host::new(vec![values.into_iter().map(frame).collect()]);
    let mut session = open(&host, Default::default());
    let mut turn = session.turn(request()).unwrap();
    let output = drain(&mut turn).unwrap();
    let result = collected(output);
    assert!(matches!(result.usage, Some(None)));
    assert!(matches!(result.user, Some(None)));
    drop(turn);
    assert!(!session.is_poisoned());
}

#[test]
fn native_tool_progress_and_child_failures_are_preserved_without_execution_or_relabeling() {
    let fixtures = vec![
        (
            json!({"type":"image_generation_call","id":"item","status":"completed","result":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC"}),
            vec![
                "response.image_generation_call.in_progress",
                "response.image_generation_call.generating",
                "response.image_generation_call.completed",
            ],
        ),
        (
            json!({"type":"code_interpreter_call","id":"item","container_id":"container","code":"print(1)","outputs":[{"type":"logs","logs":"1"}],"status":"completed"}),
            vec![
                "response.code_interpreter_call.in_progress",
                "response.code_interpreter_call.interpreting",
                "response.code_interpreter_call.completed",
            ],
        ),
        (
            json!({"type":"file_search_call","id":"item","queries":["query"],"status":"completed"}),
            vec![
                "response.file_search_call.in_progress",
                "response.file_search_call.searching",
                "response.file_search_call.completed",
            ],
        ),
        (
            json!({"type":"web_search_call","id":"item","status":"completed","action":{"type":"search","query":"query"}}),
            vec![
                "response.web_search_call.in_progress",
                "response.web_search_call.searching",
                "response.web_search_call.completed",
            ],
        ),
        (
            json!({"type":"mcp_call","id":"item","arguments":"{\"x\":1}","name":"f","server_label":"server","status":"completed","output":"result","error":null}),
            vec![
                "response.mcp_call.in_progress",
                "response.mcp_call.completed",
            ],
        ),
        (
            json!({"type":"mcp_call","id":"item","arguments":"{\"x\":1}","name":"f","server_label":"server","status":"failed","output":null,"error":"native tool error"}),
            vec!["response.mcp_call.in_progress", "response.mcp_call.failed"],
        ),
        (
            json!({"type":"mcp_list_tools","id":"item","server_label":"server","tools":[],"error":null}),
            vec![
                "response.mcp_list_tools.in_progress",
                "response.mcp_list_tools.completed",
            ],
        ),
        (
            json!({"type":"mcp_list_tools","id":"item","server_label":"server","tools":[],"error":"native discovery error"}),
            vec![
                "response.mcp_list_tools.in_progress",
                "response.mcp_list_tools.failed",
            ],
        ),
    ];
    for (item, tags) in fixtures {
        let mut body = response("r");
        body.output = serde_json::from_value(json!([item])).unwrap();
        let mut values: Vec<Value> = events(body.clone())
            .into_iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect();
        let at = values
            .iter()
            .position(|v| v["type"] == "response.output_item.done")
            .unwrap();
        values.splice(
            at..at,
            tags.iter().map(
                |tag| json!({"type":tag,"sequence_number":0,"output_index":0,"item_id":"item"}),
            ),
        );
        for (index, value) in values.iter_mut().enumerate() {
            value["sequence_number"] = json!(index);
        }
        let expected: Vec<StreamEvent> = values
            .iter()
            .cloned()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        let host = Host::new(vec![values.into_iter().map(frame).collect()]);
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        let actual = drain(&mut turn).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(collected(actual), body);
        drop(turn);
        assert!(!session.is_poisoned());
        assert!(session.failure_event().is_none());
        assert_eq!(text_sends(&host.shared), 1);
    }
}
#[test]
fn native_collector_and_adapter_caps_close_before_success() {
    for limits in [
        ResponsesWsLimits {
            collector:
                gproxy_protocol::transform::generate::stream::responses::ResponsesStreamLimits {
                    max_text_bytes: 1,
                    ..Default::default()
                },
            ..Default::default()
        },
        ResponsesWsLimits {
            max_event_bytes: 100,
            ..Default::default()
        },
        ResponsesWsLimits {
            max_frame_bytes: 128,
            ..Default::default()
        },
    ] {
        let host = Host::new(vec![frames("r")]);
        let mut session = open(&host, limits);
        let mut turn = session.turn(request()).unwrap();
        assert_eq!(
            drain(&mut turn).unwrap_err().kind(),
            TransformErrorKind::Limit
        );
        drop(turn);
        assert_closed(&host);
        assert!(session.last_status().is_none());
    }
}
#[test]
fn orphaned_audio_events_have_explicit_unsupported_policy() {
    for tag in [
        "response.audio.delta",
        "response.audio.done",
        "response.audio.transcript.delta",
        "response.audio.transcript.done",
    ] {
        let first = serde_json::to_value(&events(response("r"))[0]).unwrap();
        let mut value = json!({"type":tag,"sequence_number":1});
        if tag.ends_with("delta") {
            value["delta"] = json!("AA==");
        }
        let host = Host::new(vec![vec![frame(first), frame(value)]]);
        let mut session = open(&host, Default::default());
        let mut turn = session.turn(request()).unwrap();
        assert_eq!(
            drain(&mut turn).unwrap_err().kind(),
            TransformErrorKind::Unsupported
        );
        drop(turn);
        assert_closed(&host);
        assert!(session.last_status().is_none());
    }
}
#[test]
fn connection_errors_keep_the_original_capability_cause() {
    let mut host = Host::new(vec![]);
    host.fail_connect = true;
    let error = ready(responses_ws::connect(
        &host,
        &"t".into(),
        handshake(),
        Default::default(),
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Host);
    let cause = error
        .error
        .source()
        .unwrap()
        .downcast_ref::<CapabilityError>()
        .unwrap();
    assert_eq!(cause.kind(), CapabilityErrorKind::Transport);
    assert_eq!(cause.stage(), CapabilityErrorStage::Start);
    assert!(error.handshake.is_none());
    assert_eq!(text_sends(&host.shared), 0);
}
