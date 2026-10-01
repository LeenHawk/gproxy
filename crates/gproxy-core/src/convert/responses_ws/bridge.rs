//! Connection-owned Responses controls over HTTP streaming upstreams.
//! A logical response can contain several observed HTTP segments; control
//! acknowledgements never claim the HTTP vendor changed an in-flight request.

mod segment;

use super::*;
use gproxy_protocol::{
    adapt::generate::{
        Endpoint,
        stream::{ResponsesHistoryCache, response_output_as_input},
    },
    wire::openai::responses::{
        self as r,
        input::{Input, InputItem},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

pub(super) struct Config<C> {
    pub store: Arc<gproxy_store::Store<C>>,
    pub cache: Arc<dyn gproxy_cache::Cache>,
    pub owned: Arc<OwnedState<C>>,
    pub target: Dialect,
    pub model: String,
    pub endpoint: Endpoint,
    pub settings: StreamSettings,
    pub headers: HeaderMap,
    pub channel_state: Arc<dyn gproxy_channel::channel::ChannelState>,
    pub instance_id: Arc<str>,
    pub cancellation: CancellationToken,
}

struct Current {
    id: String,
    lane: Option<String>,
    request: r::GenerateContentRequestBody,
    transcript: Vec<InputItem>,
    output: Vec<r::response::ResponseOutputItem>,
    partial: BTreeMap<u64, String>,
    response: Value,
    sequence: i64,
    offset: u64,
    saved_output: usize,
    pending: Vec<InputItem>,
    steering: Option<(String, r::steering::SteerRequest)>,
    terminal: Option<Value>,
    usage: Option<Value>,
    usage_complete: bool,
    injected: bool,
    segments: usize,
    turn: Arc<crate::responses::Turn>,
}

struct Driver<C> {
    config: Arc<Config<C>>,
    session: crate::ResponsesSession,
    history: ResponsesHistoryCache,
    output: Outgoing,
    current: Option<Current>,
    segment: Option<segment::Segment>,
}

pub(super) fn serve<C: BatchConnectionTrait + Send + Sync + 'static>(
    config: Config<C>,
    session: crate::ResponsesSession,
    funnel: Arc<Funnel>,
    completion: UsageCompletion,
) -> WebSocketExecution {
    let (output, incoming) = mpsc::channel(FRAME_QUEUE);
    let (outgoing, mut input) = mpsc::channel(FRAME_QUEUE);
    let mut driver = Driver {
        history: ResponsesHistoryCache::new(
            1024,
            usize::try_from(config.settings.codec.max_body_bytes).unwrap_or(usize::MAX),
        ),
        config: Arc::new(config),
        session,
        output,
        current: None,
        segment: None,
    };
    let task_funnel = funnel.clone();
    crate::rt::spawn(async move {
        loop {
            tokio::select! {
                () = driver.config.cancellation.cancelled() => break,
                frame = input.recv() => {
                    match frame {
                        None => break,
                        Some(WsFrame::Close(close)) => { let _ = driver.output.send(Ok(WsFrame::Close(close))).await; break; }
                        Some(WsFrame::Ping(bytes)) => { let _ = driver.output.send(Ok(WsFrame::Pong(bytes))).await; }
                        Some(WsFrame::Pong(_)) => {},
                        Some(WsFrame::Binary(_)) => { driver.error(400, None, "binary frames are not accepted").await; }
                        Some(WsFrame::Text(text)) => driver.command(text).await,
                    }
                }
                event = async { driver.segment.as_mut().unwrap().events.recv().await }, if driver.segment.is_some() => {
                    match event {
                        Some(Ok(event)) => driver.event(event).await,
                        Some(Err((status, message))) => driver.fail(status, &message).await,
                        None => driver.segment_finished().await,
                    }
                }
            }
        }
        if let Some(segment) = driver.segment.take() {
            segment.stop().await;
        }
        driver.session.close().await;
        task_funnel.finish(UsageState::Completed).await;
    });
    let socket = WebSocket {
        incoming: Box::pin(futures_util::stream::unfold(
            incoming,
            |mut rx| async move { rx.recv().await.map(|item| (item, rx)) },
        )),
        outgoing: Box::pin(futures_util::sink::unfold(
            outgoing,
            |tx, frame| async move { tx.send(frame).await.map(|()| tx).map_err(|_| closed()) },
        )),
    };
    Execution::new(
        UpstreamConnection::Connected {
            handshake: WireResponse {
                status: StatusCode::SWITCHING_PROTOCOLS,
                headers: HeaderMap::new(),
                body: (),
            },
            socket,
        },
        completion,
        funnel.arm(),
    )
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Driver<C> {
    async fn command(&mut self, text: String) {
        let message =
            match gproxy_protocol::adapt::generate::stream::websocket::decode_session_message(
                text.as_bytes(),
                self.config.settings.codec,
            ) {
                Ok(message) => message,
                Err(error) => {
                    if self.current.is_none() {
                        self.session.finish_turn(UsageState::Failed).await;
                    }
                    self.error(400, None, &error.to_string()).await;
                    return;
                }
            };
        match message.event {
            ClientEvent::ResponseCreate(request) => {
                if self.current.is_some() {
                    self.error(
                        409,
                        message.stream_id,
                        "lane already has an active response",
                    )
                    .await;
                } else if let Err(error) = self
                    .start(request, message.stream_id, message.generate == Some(false))
                    .await
                {
                    self.fail(400, &error.to_string()).await;
                }
            }
            ClientEvent::ResponseInterrupt(request) => self.interrupt(&request.response_id).await,
            ClientEvent::ResponseSteer(request) => self.steer(request).await,
            ClientEvent::ResponseInject(request) => self.inject(request).await,
        }
    }

    async fn start(
        &mut self,
        request: r::GenerateContentRequestBody,
        lane: Option<String>,
        warmup: bool,
    ) -> Result<(), TransformError> {
        let turn = self.session.current().ok_or_else(|| {
            TransformError::shape("responses", "generation has not been admitted")
        })?;
        let state = self.config.owned.access();
        let expanded = self
            .history
            .expand(&request, &state, self.config.settings.codec)
            .await?;
        let transcript = input_items(expanded.input);
        let id = format!("resp_{}", crate::ids::random_id());
        let response = response(&request, &id);
        self.current = Some(Current {
            id,
            lane,
            request,
            transcript,
            output: vec![],
            partial: BTreeMap::new(),
            response,
            sequence: 0,
            offset: 0,
            saved_output: 0,
            pending: vec![],
            steering: None,
            terminal: None,
            usage: None,
            usage_complete: true,
            injected: false,
            segments: 0,
            turn,
        });
        self.session.response_started();
        let response = self.current.as_ref().unwrap().response.clone();
        self.emit(json!({"type":"response.created", "response":response}))
            .await;
        if warmup {
            self.complete("completed", None).await;
        } else {
            self.next_segment().await;
        }
        Ok(())
    }

    async fn next_segment(&mut self) {
        let current = self.current.as_mut().unwrap();
        if current.turn.attempt.request.snapshot.observation.settlement {
            let pending = current.turn.funnel.pending_cost().await;
            if let Err(error) = crate::budget::check_pending(
                &self.config.store,
                &current.turn.attempt.request,
                now_ms(),
                pending.as_ref(),
            )
            .await
            {
                self.fail(429, &error.to_string()).await;
                return;
            }
        }
        current.offset = current.output.len() as u64;
        if current.segments > 0
            && let Err(error) = crate::quota::charge_responses_segment(
                &self.config.store,
                &self.config.cache,
                &current.turn.attempt,
                Operation::StreamGenerateContent,
            )
            .await
        {
            self.fail(429, &error.to_string()).await;
            return;
        }
        current.segments += 1;
        current.terminal = None;
        current.injected = false;
        let mut input = current.request.clone();
        input.input = Some(Input::Items(current.transcript.clone()));
        input.previous_response_id = None;
        self.segment = Some(segment::start(
            self.config.clone(),
            current.turn.clone(),
            input,
        ));
    }

    async fn event(&mut self, mut event: Value) {
        let Some(current) = &mut self.current else {
            return;
        };
        let kind = event["type"].as_str().unwrap_or("").to_owned();
        if kind == "response.created" {
            return;
        }
        if matches!(
            kind.as_str(),
            "response.completed" | "response.incomplete" | "response.failed"
        ) {
            current.terminal = Some(event);
            return;
        }
        if kind == "error" {
            let message = event["message"]
                .as_str()
                .or_else(|| event["error"]["message"].as_str())
                .unwrap_or("upstream stream error")
                .to_owned();
            self.fail(502, &message).await;
            return;
        }
        if let Some(index) = event["output_index"].as_u64() {
            event["output_index"] = (index + current.offset).into();
        }
        if kind == "response.output_item.added" || kind == "response.output_item.done" {
            mark_async(&current.request, &mut event["item"]);
            if let (Some(index), Some(id)) =
                (event["output_index"].as_u64(), event["item"]["id"].as_str())
            {
                if kind.ends_with("added") {
                    current.partial.insert(index, id.into());
                } else {
                    current.partial.remove(&index);
                }
            }
            if kind.ends_with("done") {
                match serde_json::from_value::<r::response::ResponseOutputItem>(
                    event["item"].clone(),
                ) {
                    Ok(item) => current.output.push(item),
                    Err(error) => {
                        self.fail(502, &error.to_string()).await;
                        return;
                    }
                }
            }
        }
        self.emit(event).await;
    }

    async fn segment_finished(&mut self) {
        if let Some(segment) = self.segment.take() {
            segment.stop().await;
        }
        let Some(current) = &mut self.current else {
            return;
        };
        let Some(terminal) = current.terminal.take() else {
            self.fail(502, "upstream ended without a Responses terminal event")
                .await;
            return;
        };
        let status = terminal["response"]["status"]
            .as_str()
            .unwrap_or("failed")
            .to_owned();
        if let Some(usage) = terminal["response"]["usage"].as_object() {
            add_usage(&mut current.usage, &Value::Object(usage.clone()));
        } else {
            current.usage_complete = false;
        }
        if let Err(error) = current.save_output() {
            self.fail(502, &error.to_string()).await;
            return;
        }
        if status != "completed" {
            current.response["error"] = terminal["response"]["error"].clone();
            let reason = terminal["response"]["incomplete_details"]["reason"]
                .as_str()
                .map(str::to_owned);
            self.complete(&status, reason.as_deref()).await;
            return;
        }
        current.transcript.append(&mut current.pending);
        if !current.required(true).is_empty() {
            return;
        }
        // A segment with accepted injections continues the same response.
        if current.injected {
            self.next_segment().await;
        } else {
            self.complete("completed", None).await;
        }
    }

    async fn interrupt(&mut self, response_id: &str) {
        if !self.matches(response_id) {
            self.error(400, None, "response_not_found").await;
            return;
        }
        self.emit(json!({"type":"response.interrupt.accepted","response_id":response_id}))
            .await;
        self.stop_partial().await;
        self.reject_steering("response interrupted").await;
        self.complete("incomplete", Some("interrupted")).await;
    }

    async fn steer(&mut self, request: r::steering::SteerRequest) {
        if !self.matches(&request.previous_response_id) {
            self.session.reject_successor().await;
            self.error(400, None, "response_not_found").await;
            return;
        }
        let id = format!("steer_{}", crate::ids::random_id());
        self.emit(json!({"type":"response.steer.accepted","steer":{
            "id":id,"previous_response_id":request.previous_response_id
        }}))
        .await;
        self.stop_partial().await;
        let current = self.current.as_mut().unwrap();
        current.steering = Some((id, request));
        let required = current.required(false);
        if required.is_empty() {
            self.continue_steering().await;
        } else {
            let (id, request) = current.steering.as_ref().unwrap();
            let event = json!({"type":"response.steer.pending","steer":{"id":id,"previous_response_id":request.previous_response_id},
                "reason":"tool_results_required","required_input":required});
            self.emit(event).await;
        }
    }

    async fn inject(&mut self, request: r::multi_agent::InjectRequest) {
        if !self.matches(&request.response_id) {
            self.error(400, None, "response_not_found").await;
            return;
        }
        let current = self.current.as_mut().unwrap();
        let required = current.required(false);
        for item in &request.input {
            let item = serde_json::to_value(item).unwrap();
            if let Some(call_id) = item["call_id"].as_str()
                && item["type"]
                    .as_str()
                    .is_some_and(|t| t.ends_with("_output"))
                && !required.iter().any(|r| r["call_id"] == call_id)
            {
                self.error(400, None, "tool result does not match an outstanding call")
                    .await;
                return;
            }
        }
        current.pending.extend(request.input);
        current.injected = true;
        self.emit(json!({"type":"response.inject.created","response_id":request.response_id}))
            .await;
        if self.segment.is_none() {
            let current = self.current.as_mut().unwrap();
            current.transcript.append(&mut current.pending);
            if current.required(false).is_empty() {
                if current.steering.is_some() {
                    self.continue_steering().await;
                } else {
                    self.next_segment().await;
                }
            }
        }
    }

    async fn continue_steering(&mut self) {
        let current = self.current.as_mut().unwrap();
        let (_, steer) = current.steering.as_ref().unwrap().clone();
        let mut request = current.request.clone();
        request.previous_response_id = Some(Some(current.id.clone()));
        request.input = Some(match steer.input {
            r::steering::SteerInput::Text(text) => Input::Text(text),
            r::steering::SteerInput::Messages(messages) => Input::Items(
                messages
                    .into_iter()
                    .map(|m| {
                        InputItem::Easy(
                            r::input::EasyInputMessage::builder(
                                m.content,
                                r::input::MessageRole::User,
                            )
                            .build(),
                        )
                    })
                    .collect(),
            ),
        });
        let lane = current.lane.clone();
        if !self.complete("incomplete", Some("steered")).await {
            return;
        }
        if let Err(error) = self.start(request, lane, false).await {
            self.fail(400, &error.to_string()).await;
        }
    }

    async fn stop_partial(&mut self) {
        if let Some(segment) = self.segment.take() {
            segment.stop().await;
        }
        let current = self.current.as_mut().unwrap();
        current.usage_complete = false;
        let _ = current.save_output();
        let interrupted = std::mem::take(&mut current.partial);
        let response_id = current.id.clone();
        for (index, item_id) in interrupted {
            self.emit(json!({"type":"response.output_item.interrupted","response_id":response_id,"item_id":item_id,"output_index":index})).await;
        }
    }

    async fn complete(&mut self, status: &str, reason: Option<&str>) -> bool {
        let Some(current) = &mut self.current else {
            return false;
        };
        if let Err(error) = current.save_output() {
            self.fail(502, &error.to_string()).await;
            return false;
        }
        let state = self.config.owned.access();
        if let Err(error) = self
            .history
            .save_transcript(
                &current.id,
                current.transcript.clone(),
                current.request.store.flatten() != Some(false),
                &state,
                self.config.settings.codec,
            )
            .await
        {
            self.fail(500, &error.to_string()).await;
            return false;
        }
        let current = self.current.as_mut().unwrap();
        let mut response = current.response.clone();
        response["status"] = status.into();
        response["output"] = serde_json::to_value(&current.output).unwrap();
        response["incomplete_details"] = reason
            .map(|reason| json!({"reason":reason}))
            .unwrap_or(Value::Null);
        response["usage"] = if current.usage_complete {
            current.usage.clone().unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        let event = json!({"type":format!("response.{status}"),"response":response});
        // Settlement must finish before the terminal reaches a host awaiting it.
        self.session
            .finish_turn(if status == "failed" {
                UsageState::Failed
            } else {
                UsageState::Completed
            })
            .await;
        self.emit(event).await;
        self.current = None;
        true
    }

    async fn fail(&mut self, status: u16, message: &str) {
        if let Some(segment) = self.segment.take() {
            segment.stop().await;
        }
        self.reject_steering(message).await;
        if let Some(current) = &self.current {
            let mut response = current.response.clone();
            response["status"] = "failed".into();
            response["output"] = serde_json::to_value(&current.output).unwrap();
            response["error"] = json!({"code":"server_error","message":message});
            self.session.finish_turn(UsageState::Failed).await;
            self.emit(json!({"type":"response.failed","response":response}))
                .await;
            self.current = None;
        } else {
            self.session.finish_turn(UsageState::Failed).await;
            self.error(status, None, message).await;
        }
    }

    async fn reject_steering(&mut self, message: &str) {
        let steering = self.current.as_mut().and_then(|c| c.steering.take());
        self.session.cancel_pending().await;
        if let Some((id, request)) = steering {
            self.emit(json!({"type":"response.steer.failed","steer":{"id":id,"previous_response_id":request.previous_response_id,"input":request.input},
                "error":{"type":"invalid_request_error","code":"interrupted","message":message}})).await;
        }
    }

    fn matches(&self, id: &str) -> bool {
        self.current.as_ref().is_some_and(|c| c.id == id)
    }

    async fn emit(&mut self, mut event: Value) {
        let Some(current) = &mut self.current else {
            return;
        };
        if event.get("response_id").is_some() {
            event["response_id"] = current.id.clone().into();
        }
        if let Some(response) = event.get_mut("response").and_then(Value::as_object_mut) {
            response.insert("id".into(), current.id.clone().into());
        }
        event["sequence_number"] = current.sequence.into();
        current.sequence += 1;
        if let Some(lane) = &current.lane {
            event["stream_id"] = lane.clone().into();
        }
        // NativeCall already applies provider rules to the selected upstream
        // dialect. Applying the WS rules again would duplicate broad rules.
        let _ = self.output.send(Ok(WsFrame::Text(event.to_string()))).await;
    }

    async fn error(&mut self, status: u16, lane: Option<String>, message: &str) {
        let lane = lane.or_else(|| self.current.as_ref().and_then(|c| c.lane.clone()));
        let _ = self
            .output
            .send(Ok(error_frame(status, lane, message.into())))
            .await;
    }
}

impl Current {
    fn save_output(&mut self) -> Result<(), TransformError> {
        for item in self.output[self.saved_output..].iter().cloned() {
            self.transcript.push(response_output_as_input(item)?);
        }
        self.saved_output = self.output.len();
        Ok(())
    }

    fn required(&self, asynchronous_only: bool) -> Vec<Value> {
        let mut required = vec![];
        let results: Vec<_> = self
            .transcript
            .iter()
            .chain(&self.pending)
            .map(|i| serde_json::to_value(i).unwrap())
            .collect();
        for item in &self.output {
            let item = serde_json::to_value(item).unwrap();
            let Some(call_id) = item["call_id"].as_str() else {
                continue;
            };
            let kind = match item["type"].as_str() {
                Some("function_call") => "function_call_output",
                Some("custom_tool_call") => "custom_tool_call_output",
                _ => continue,
            };
            if asynchronous_only && item["async"] != true {
                continue;
            }
            if results
                .iter()
                .any(|r| r["type"] == kind && r["call_id"] == call_id)
            {
                continue;
            }
            let mut entry = json!({"type":kind,"call_id":call_id});
            if kind == "function_call_output" {
                entry["name"] = item["name"].clone();
            }
            required.push(entry);
        }
        required
    }
}

fn input_items(input: Option<Input>) -> Vec<InputItem> {
    match input {
        Some(Input::Items(items)) => items,
        Some(Input::Text(text)) => vec![InputItem::Easy(
            r::input::EasyInputMessage::builder(
                r::input::MessageContent::Text(text),
                r::input::MessageRole::User,
            )
            .build(),
        )],
        _ => vec![],
    }
}

fn response(input: &r::GenerateContentRequestBody, id: &str) -> Value {
    let input = serde_json::to_value(input).unwrap();
    let mut response = json!({"id":id,"object":"response","created_at":now_ms().div_euclid(1000),"status":"in_progress",
        "error":null,"incomplete_details":null,"instructions":null,"metadata":null,"model":input["model"],"output":[],
        "parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"truncation":"disabled",
        "usage":null,"user":null});
    for field in [
        "instructions",
        "metadata",
        "model",
        "parallel_tool_calls",
        "temperature",
        "tool_choice",
        "tools",
        "top_p",
        "truncation",
        "store",
        "service_tier",
        "max_output_tokens",
        "reasoning",
        "text",
    ] {
        if let Some(value) = input.get(field) {
            response[field] = value.clone();
        }
    }
    response
}

fn mark_async(request: &r::GenerateContentRequestBody, item: &mut Value) {
    let Some(name) = item["name"].as_str() else {
        return;
    };
    let tools = serde_json::to_value(&request.tools).unwrap();
    if tools.as_array().is_some_and(|tools| {
        tools
            .iter()
            .any(|t| t["name"] == name && t["async"] == true)
    }) {
        item["async"] = true.into();
    }
}

fn add_usage(total: &mut Option<Value>, value: &Value) {
    fn add(total: &mut Value, value: &Value) {
        match (total, value) {
            (Value::Object(total), Value::Object(value)) => {
                for (key, value) in value {
                    match total.get_mut(key) {
                        Some(total) => add(total, value),
                        None => {
                            total.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            (Value::Number(total), Value::Number(value)) => {
                if let (Some(a), Some(b)) = (total.as_u64(), value.as_u64()) {
                    *total = a.saturating_add(b).into();
                }
            }
            _ => {}
        }
    }
    match total {
        Some(total) => add(total, value),
        None => *total = Some(value.clone()),
    }
}
