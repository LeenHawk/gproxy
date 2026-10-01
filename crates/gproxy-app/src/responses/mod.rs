//! Responses sessions authenticate before upgrading and admit each generation
//! after its model arrives. Realtime and vendor-service sockets stay separate.

mod lane;

use std::{collections::HashMap, sync::Arc};

use futures_util::StreamExt;
use gproxy_protocol::connection::{TransportError, WebSocket, WsFrame};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    App, AppError, Caller, CaptureDirection, CaptureOutcome, CapturedFrame, DataPlaneRequest,
    DownstreamCapture,
};

const QUEUE: usize = 16;
type Lane = Option<String>;

struct Message {
    value: Value,
    text: String,
    request_id: String,
}

enum Event {
    Frame {
        lane: Lane,
        turn: Option<String>,
        frame: WsFrame,
    },
    Finished {
        capture: Option<Box<DownstreamCapture>>,
        outcome: CaptureOutcome,
    },
}

struct Context<C> {
    app: Arc<App<C>>,
    caller: Caller,
    request: DataPlaneRequest,
    prefix: Option<String>,
    cancellation: CancellationToken,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// No generation or upstream handshake is started here. The socket can
    /// receive a model-specific refusal and then another legitimate request.
    pub fn open_responses(
        self: &Arc<Self>,
        caller: Caller,
        request: DataPlaneRequest,
        prefix: Option<String>,
    ) -> Result<WebSocket, AppError> {
        crate::admission::permission::check_oauth_operation(
            &caller,
            request.operation.operation,
            &self.config().oauth.cli_client_ids,
        )?;
        let cancellation = request.cancellation.clone().unwrap_or_default();
        let mut capture =
            DownstreamCapture::open_responses(&self.data().observation, &request, &caller);
        if let Some(capture) = &mut capture {
            capture.record_response_head(
                http::StatusCode::SWITCHING_PROTOCOLS,
                &http::HeaderMap::new(),
            );
        }
        let context = Arc::new(Context {
            app: self.clone(),
            caller,
            request,
            prefix,
            cancellation,
        });
        let (to_task, from_client) = mpsc::channel(QUEUE);
        let (to_client, from_task) = mpsc::channel(QUEUE);
        crate::rt::spawn(run(context, from_client, to_client, capture));
        Ok(WebSocket {
            incoming: Box::pin(futures_util::stream::unfold(
                from_task,
                |mut rx| async move { rx.recv().await.map(|frame| (Ok(frame), rx)) },
            )),
            outgoing: Box::pin(futures_util::sink::unfold(
                to_task,
                |tx, frame| async move { tx.send(frame).await.map(|()| tx).map_err(|_| closed()) },
            )),
        })
    }
}

async fn run<C: BatchConnectionTrait + Send + Sync + 'static>(
    context: Arc<Context<C>>,
    mut input: mpsc::Receiver<WsFrame>,
    output: mpsc::Sender<WsFrame>,
    mut capture: Option<DownstreamCapture>,
) {
    let (events, mut receive) = mpsc::channel(QUEUE);
    let mut lanes: HashMap<Lane, mpsc::Sender<Message>> = HashMap::new();
    let mut owners: HashMap<String, Vec<Lane>> = HashMap::new();
    let mut tasks = futures_util::stream::FuturesUnordered::<
        gproxy_protocol::capability::CapabilityFuture<'static, ()>,
    >::new();
    let mut close = None;
    let mut outcome = CaptureOutcome::Complete;
    loop {
        tokio::select! {
            () = context.cancellation.cancelled() => { outcome = CaptureOutcome::Cancelled; break; }
            _ = tasks.next(), if !tasks.is_empty() => {}
            event = receive.recv() => {
                if let Some(event) = event
                    && !deliver(event, &output, &mut capture, &mut owners).await { break; }
            }
            frame = input.recv() => {
                let Some(frame) = frame else { outcome = CaptureOutcome::Cancelled; break };
                record(&mut capture, CaptureDirection::Request, &frame, None);
                let mut text = match frame {
                    WsFrame::Close(value) => { close = value; break; }
                    WsFrame::Ping(bytes) => { let _ = output.send(WsFrame::Pong(bytes)).await; continue; }
                    WsFrame::Pong(_) => continue,
                    WsFrame::Binary(_) => {
                        if output.send(error(400, None, "invalid_request", "Responses requires JSON text messages")).await.is_err() { break; }
                        continue;
                    }
                    WsFrame::Text(text) => text,
                };
                let parsed = parse(&text);
                let (mut value, mut lane) = match parsed {
                    Ok(parsed) => parsed,
                    Err(message) => {
                        if output.send(error(400, None, "invalid_request", &message)).await.is_err() { break; }
                        continue;
                    }
                };
                let kind = value["type"].as_str().unwrap();
                if kind != "response.create" {
                    let id = value["response_id"].as_str().or_else(|| value["previous_response_id"].as_str());
                    if let Some(id) = id {
                        let found = owners.get(id).filter(|found| {
                            lane.as_ref().is_none_or(|_| found.contains(&lane))
                        });
                        match found {
                            Some(found) if lane.is_some() => { let _ = found; }
                            Some(found) if found.len() == 1 => lane = found[0].clone(),
                            _ => {
                                let _ = output.send(control_error(&value, lane.as_deref(), "response_not_found", "response is not owned by this lane", 0)).await;
                                continue;
                            }
                        }
                    }
                    if let Some(lane) = &lane && value.get("stream_id").is_none_or(Value::is_null) {
                        value["stream_id"] = lane.clone().into();
                        text = value.to_string();
                    }
                }
                let sender = lanes.entry(lane.clone()).or_insert_with(|| {
                    let (tx, rx) = mpsc::channel(QUEUE);
                    tasks.push(Box::pin(lane::run(context.clone(), lane.clone(), rx, events.clone())));
                    tx
                });
                let message = Message { value, text, request_id: random_id() };
                // A saturated lane must not block control/traffic on other lanes.
                if sender.try_send(message).is_err() {
                    let _ = output.send(error(429, lane.as_deref(), "lane_busy", "Responses lane is busy")).await;
                }
            }
        }
    }
    context.cancellation.cancel();
    drop(lanes);
    drop(events);
    // Workers finish their usage and logs before the parent connection row.
    loop {
        let event = tokio::select! {
            event = receive.recv() => match event { Some(event) => event, None => break },
            _ = tasks.next(), if !tasks.is_empty() => continue,
        };
        if let Event::Finished {
            capture: Some(turn),
            outcome,
        } = event
            && let Some(parent) = &mut capture
        {
            parent.append_turn(*turn, outcome);
        }
    }
    while tasks.next().await.is_some() {}
    if let Some(capture) = capture {
        let _ = capture.finish(context.app.gproxy().store(), outcome).await;
    }
    let _ = output.send(WsFrame::Close(close)).await;
}

async fn deliver(
    event: Event,
    output: &mpsc::Sender<WsFrame>,
    capture: &mut Option<DownstreamCapture>,
    owners: &mut HashMap<String, Vec<Lane>>,
) -> bool {
    match event {
        Event::Finished {
            capture: Some(turn),
            outcome,
        } => {
            if let Some(parent) = capture {
                parent.append_turn(*turn, outcome);
            }
        }
        Event::Finished { capture: None, .. } => {}
        Event::Frame { lane, turn, frame } => {
            if let WsFrame::Text(text) = &frame
                && let Ok(value) = serde_json::from_str::<Value>(text)
                && let Some(id) = value.get("response").and_then(|r| r["id"].as_str())
            {
                let found = owners.entry(id.into()).or_default();
                if !found.contains(&lane) {
                    found.push(lane);
                }
            }
            record(capture, CaptureDirection::Response, &frame, turn.as_deref());
            return output.send(frame).await.is_ok();
        }
    }
    true
}

fn record(
    capture: &mut Option<DownstreamCapture>,
    direction: CaptureDirection,
    frame: &WsFrame,
    turn: Option<&str>,
) {
    let Some(capture) = capture else { return };
    let frame = match frame {
        WsFrame::Text(text) => CapturedFrame::Text(text),
        WsFrame::Binary(bytes) => CapturedFrame::Binary(bytes),
        WsFrame::Ping(bytes) => CapturedFrame::Ping(bytes),
        WsFrame::Pong(bytes) => CapturedFrame::Pong(bytes),
        WsFrame::Close(value) => CapturedFrame::Close {
            code: value.as_ref().map(|v| v.code),
            reason: value.as_ref().map(|v| v.reason.as_str()).unwrap_or(""),
        },
    };
    capture.record_turn_frame(direction, frame, turn);
}

fn parse(text: &str) -> Result<(Value, Lane), String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if value["type"].as_str().is_none() {
        return Err("message type is required".into());
    }
    let lane = match value.get("stream_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(lane))
            if !lane.is_empty()
                && lane.len() <= 256
                && lane
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)) =>
        {
            Some(lane.clone())
        }
        _ => return Err("invalid stream_id".into()),
    };
    Ok((value, lane))
}

fn error(status: u16, lane: Option<&str>, code: &str, message: &str) -> WsFrame {
    let mut value = json!({"type":"error","status":status,"error":{
        "type":"invalid_request_error","code":code,"message":message
    }});
    if let Some(lane) = lane {
        value["stream_id"] = lane.into();
    }
    WsFrame::Text(value.to_string())
}

fn app_error(lane: Option<&str>, error: &AppError) -> WsFrame {
    if let AppError::Sdk(gproxy_sdk::SdkError::Upstream { status, body }) = error
        && let Ok(value) = serde_json::from_str::<Value>(body)
        && value["error"]["message"].is_string()
    {
        let mut event = json!({"type":"error","status":status,"error":value["error"]});
        if let Some(lane) = lane {
            event["stream_id"] = lane.into();
        }
        return WsFrame::Text(event.to_string());
    }
    self::error(error.status_code(), lane, error.code(), &error.to_string())
}

fn control_error(
    request: &Value,
    lane: Option<&str>,
    code: &str,
    message: &str,
    sequence: i64,
) -> WsFrame {
    let mut event = match request["type"].as_str() {
        Some("response.inject") if request["response_id"].is_string() => json!({
            "type":"response.inject.failed", "sequence_number":sequence,
            "response_id":request["response_id"], "input":request["input"],
            "error":{"code":code,"message":message}
        }),
        Some("response.steer") if request["previous_response_id"].is_string() => json!({
            "type":"response.steer.failed", "sequence_number":sequence,
            "steer":{"previous_response_id":request["previous_response_id"],"input":request["input"]},
            "error":{"type":"invalid_request_error","code":code,"message":message}
        }),
        _ => return error(400, lane, code, message),
    };
    if let Some(lane) = lane {
        event["stream_id"] = lane.into();
    }
    WsFrame::Text(event.to_string())
}

fn closed() -> TransportError {
    Box::new(std::io::Error::other("Responses connection closed"))
}

fn random_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("system random source");
    crate::hex::encode(&bytes)
}
