// Reuse the native duplex host and its contract assertions.
include!("../adapt_responses_ws.rs");
use super::{all_pairs, host as http_host, settings};
use gproxy_protocol::{
    Dialect,
    adapt::generate::{
        self as generation,
        stream::{self as runtime, bridge::StreamBridge},
    },
    transform::identity::IdentityTarget,
};
fn access(
    store: &http_host::Store,
    native: Dialect,
) -> generation::GenerationStateAccess<'_, http_host::Store> {
    let mut state = all_pairs::access(store, native);
    state.target = IdentityTarget::new("selected", native)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    state
}
fn ws_body() -> r::GenerateContentResponseBody {
    serde_json::from_value(all_pairs::native_response(Dialect::OpenAi)).unwrap()
}
fn ws_open(
    host: &Host,
    state: &generation::GenerationStateAccess<'_, http_host::Store>,
) -> runtime::websocket::GenerationWsSession {
    match http_host::ready(runtime::websocket::connect(
        host,
        &"selected-origin".to_string(),
        handshake(),
        ResponsesWsLimits::default(),
        state,
    ))
    .unwrap()
    {
        runtime::websocket::GenerationWsConnect::Connected { session, .. } => session,
        _ => panic!("unexpected rejection"),
    }
}
fn run_source<B>(mut call: runtime::StreamInvocation<B>, store: &http_host::Store)
where
    B: StreamBridge<NativeEvent = StreamEvent, NativeRequest = GenerateContentRequestBody>,
{
    let state = access(store, Dialect::OpenAi);
    let host = Host::new(vec![frames_for(ws_body())]);
    let mut session = ws_open(&host, &state);
    let mut turn = http_host::ready(call.start_websocket(&mut session, &state)).unwrap();
    assert!(
        !store
            .entries
            .lock()
            .unwrap()
            .keys()
            .any(|key| http_host::is_reservation(key))
    );
    assert_eq!(text_sends(&host.shared), 0);
    let first = http_host::ready(turn.next(&state)).unwrap().unwrap();
    assert!(first.event.is_some());
    assert!(turn.invocation().client_result().is_none());
    assert!(!host.shared.lock().unwrap().eof);
    let mut count = 1;
    while let Some(chunk) = http_host::ready(turn.next(&state)).unwrap() {
        count += usize::from(chunk.event.is_some());
    }
    assert!(count > 2);
    assert!(turn.invocation().client_result().is_some());
    assert!(turn.invocation().native_result().is_some());
    drop(turn);
    assert!(!session.native().is_poisoned());
    assert_eq!(session.native().last_response_id(), Some("resp:source"));
    assert_eq!(text_sends(&host.shared), 1);
    let sent = host.shared.lock().unwrap();
    let WsFrame::Text(text) = &sent.sent[0] else {
        panic!()
    };
    let request: Value = serde_json::from_str(text).unwrap();
    assert_eq!(request["type"], "response.create");
    assert!(request.get("stream").is_none());
}
#[test]
fn ws_responses_to_chat_actual_incremental_host() {
    let store = http_host::Store::default();
    let state = access(&store, Dialect::OpenAi);
    let call = http_host::ready(
        generation::chat_responses::ChatViaResponses::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::OpenAiChat)).unwrap(),
            all_pairs::target(Dialect::OpenAiChat, Dialect::OpenAi),
            settings(),
            &state,
        ),
    )
    .unwrap();
    run_source(call, &store);
}
#[test]
fn ws_responses_to_claude_actual_incremental_host() {
    let store = http_host::Store::default();
    let state = access(&store, Dialect::OpenAi);
    let context = gproxy_protocol::transform::generate::claude_responses::stream::ResponsesToClaudeContext {
        usage: Some(serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap()), ..Default::default()
    };
    let call = http_host::ready(
        generation::claude_responses::ClaudeViaResponses::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::Claude)).unwrap(),
            all_pairs::target(Dialect::Claude, Dialect::OpenAi),
            context,
            settings(),
            &state,
        ),
    )
    .unwrap();
    run_source(call, &store);
}
#[test]
fn ws_responses_to_gemini_actual_incremental_host() {
    let store = http_host::Store::default();
    let state = access(&store, Dialect::OpenAi);
    let call = http_host::ready(
        generation::gemini_responses::GeminiViaResponses::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::Gemini)).unwrap(),
            all_pairs::target(Dialect::Gemini, Dialect::OpenAi),
            Default::default(),
            settings(),
            &state,
        ),
    )
    .unwrap();
    run_source(call, &store);
}
fn run_client<B>(
    mut call: runtime::StreamInvocation<B>,
    store: Arc<http_host::Store>,
    native: Dialect,
) -> r::GenerateContentResponseBody
where
    B: StreamBridge<ClientEvent = StreamEvent>,
{
    let state = access(&store, native);
    let feed = http_host::Feed::default();
    all_pairs::source(&feed, native);
    feed.close();
    let host = http_host::Host::stream(store.clone(), feed);
    call.responses_websocket_output().unwrap();
    http_host::ready(call.start(&host, &(), &state)).unwrap();
    let mut collector = ResponsesStreamCollector::new(Default::default());
    let mut count = 0;
    while let Some(chunk) = http_host::ready(call.next(&state)).unwrap() {
        if let Some(event) = chunk.event {
            let parsed: StreamEvent = serde_json::from_slice(&chunk.bytes).unwrap();
            assert_eq!(parsed, event);
            collector.push(parsed).unwrap();
            count += 1;
            if matches!(
                event,
                StreamEvent::Completed(_) | StreamEvent::Incomplete(_)
            ) {
                assert!(
                    store
                        .entries
                        .lock()
                        .unwrap()
                        .keys()
                        .any(|key| key.starts_with("responses-history:")),
                    "history must be durable before client terminal exposure"
                );
            }
        } else {
            assert!(chunk.bytes.is_empty());
            assert!(chunk.finished);
        }
    }
    assert!(count > 2);
    let final_response = collector.finish().unwrap().value;
    assert_eq!(call.client_result(), Some(&final_response));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    final_response
}
fn client_request() -> GenerateContentRequestBody {
    let mut value = all_pairs::request(Dialect::OpenAi);
    value["type"] = json!("response.create");
    value["unknown"] = json!({"nested":"DROP"});
    runtime::websocket::decode_request(&serde_json::to_vec(&value).unwrap(), settings().codec)
        .unwrap()
}
fn continuation(response: r::GenerateContentResponseBody) -> GenerateContentRequestBody {
    let call_id = response
        .output
        .into_iter()
        .find_map(|item| match item {
            r::ResponseOutputItem::FunctionCall(call) => Some(call.call_id),
            _ => None,
        })
        .unwrap();
    let mut req = client_request();
    req.previous_response_id = Some(Some(response.id));
    req.input = Some(serde_json::from_value(json!([{"type":"function_call_output","call_id":call_id,"output":"actual tool result"}])).unwrap());
    req
}
macro_rules! client_case {
    ($name:ident, $adapter:ty, $native:expr, $context:expr) => {
        #[test]
        fn $name() {
            let store = Arc::new(http_host::Store::default());
            let state = access(&store, $native);
            let call = http_host::ready(<$adapter>::prepare_stream(
                client_request(),
                all_pairs::target(Dialect::OpenAi, $native),
                $context,
                settings(),
                &state,
            ))
            .unwrap();
            let final_response = run_client(call, store.clone(), $native);
            let request = continuation(final_response);
            let mut target = all_pairs::target(Dialect::OpenAi, $native);
            target.identities = generation::GenerationIdentity::new(
                IdNamespace::with_bytes([91; 16]),
                IdNamespace::with_bytes([92; 16]),
                Dialect::OpenAi,
                $native,
            )
            .unwrap();
            let second = http_host::ready(<$adapter>::prepare_stream(
                request.clone(),
                target,
                $context,
                settings(),
                &state,
            ))
            .unwrap();
            assert_eq!(
                second.original_request().previous_response_id,
                request.previous_response_id
            );
            let prepared = serde_json::to_string(second.target_request()).unwrap();
            for text in ["hi", "answer", "actual tool result", "tool:source"] {
                assert!(prepared.contains(text), "missing {text}: {prepared}");
            }
            assert!(!prepared.contains("previous_response_id"));
            assert!(!prepared.contains("DROP"));
        }
    };
}
client_case!(
    ws_chat_to_responses_and_scoped_continuation,
    generation::chat_responses::ResponsesViaChat,
    Dialect::OpenAiChat,
    all_pairs::chat_response_context()
);
client_case!(
    ws_claude_to_responses_and_scoped_continuation,
    generation::claude_responses::ResponsesViaClaude,
    Dialect::Claude,
    runtime::ResponsesViaClaudeStreamFacts {
        request: Default::default(),
        response: all_pairs::claude_response_context()
    }
);
client_case!(
    ws_gemini_to_responses_and_scoped_continuation,
    generation::gemini_responses::ResponsesViaGemini,
    Dialect::Gemini,
    runtime::ResponsesViaGeminiStreamFacts {
        request: Default::default(),
        response: all_pairs::gemini_response_context()
    }
);
#[test]
fn ws_named_lane_errors_and_malformed_known_errors_are_explicit() {
    use gproxy_protocol::wire::openai::responses::websocket::{ClientEvent, RequestMessage};
    let error = json!({"type":"error","status":400,"stream_id":"main","error":{"type":"invalid_request_error","code":"previous_response_not_found","message":"gone","param":"previous_response_id","unknown":"DROP"},"unknown":"DROP"});
    let host = Host::new(vec![vec![frame(error)]]);
    let mut session = open(&host, ResponsesWsLimits::default());
    let mut turn = session
        .turn_message(RequestMessage {
            stream_id: Some("main".into()),
            generate: None,
            event: ClientEvent::ResponseCreate(request()),
        })
        .unwrap();
    assert!(drain(&mut turn).is_err());
    let failure = turn.websocket_failure().unwrap();
    assert_eq!(failure.status, 400);
    assert_eq!(failure.stream_id.as_deref(), Some("main"));
    assert_eq!(
        failure.error.code.as_deref(),
        Some("previous_response_not_found")
    );
    assert!(!serde_json::to_string(failure).unwrap().contains("DROP"));
    drop(turn);
    assert!(session.is_poisoned());
    let malformed = json!({"type":"error","status":400,"error":{"message":123},"code":null,"message":"SSE fallback must not swallow malformed WS error","param":null,"sequence_number":0});
    let host = Host::new(vec![vec![frame(malformed)]]);
    let mut session = open(&host, ResponsesWsLimits::default());
    let mut turn = session.turn(request()).unwrap();
    assert!(drain(&mut turn).is_err());
    assert!(turn.failure_event().is_none());
    assert!(turn.websocket_failure().is_none());
}
#[test]
fn ws_named_lane_native_messages_reuse_connection_and_strip_unused_controls() {
    use gproxy_protocol::wire::openai::responses::websocket::{ClientEvent, RequestMessage};
    let mut prefill = response("resp_lane");
    prefill.output.clear();
    let batch = events(prefill)
        .into_iter()
        .map(|event| {
            let mut value = serde_json::to_value(event).unwrap();
            value["stream_id"] = json!("main");
            frame(value)
        })
        .collect();
    let host = Host::new(vec![batch]);
    let mut session = open(&host, ResponsesWsLimits::default());
    let mut req = request();
    req.stream = Some(Some(false));
    req.background = Some(Some(false));
    let mut turn = session
        .turn_message(RequestMessage {
            stream_id: Some("main".into()),
            generate: Some(false),
            event: ClientEvent::ResponseCreate(req),
        })
        .unwrap();
    assert!(drain(&mut turn).is_ok());
    drop(turn);
    assert!(!session.is_poisoned());
    let shared = host.shared.lock().unwrap();
    let WsFrame::Text(text) = &shared.sent[0] else {
        panic!()
    };
    let request: Value = serde_json::from_str(text).unwrap();
    drop(shared);
    assert_eq!(request["stream_id"], "main");
    assert_eq!(request["generate"], false);
    assert!(request.get("stream").is_none());
    assert!(request.get("background").is_none());
    let prefill = br#"{"type":"response.create","generate":false,"input":"hi"}"#;
    assert_eq!(
        runtime::websocket::decode_message(prefill, settings().codec)
            .unwrap_err()
            .kind(),
        TransformErrorKind::Unsupported
    );
}
#[test]
fn ws_generation_cancel_before_poll_never_repeats_send_or_reuses_connection() {
    let store = http_host::Store::default();
    let state = access(&store, Dialect::OpenAi);
    let mut call = http_host::ready(
        generation::chat_responses::ChatViaResponses::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::OpenAiChat)).unwrap(),
            all_pairs::target(Dialect::OpenAiChat, Dialect::OpenAi),
            settings(),
            &state,
        ),
    )
    .unwrap();
    let host = Host::new(vec![frames_for(ws_body())]);
    let mut session = ws_open(&host, &state);
    let turn = http_host::ready(call.start_websocket(&mut session, &state)).unwrap();
    drop(turn);
    assert!(session.native().is_poisoned());
    assert_eq!(text_sends(&host.shared), 0);
    assert!(http_host::ready(call.start_websocket(&mut session, &state)).is_err());
    assert_eq!(text_sends(&host.shared), 0);
}
#[test]
fn ws_store_false_uses_connection_cache_and_reconnect_requires_full_history() {
    let store = Arc::new(http_host::Store::default());
    let state = access(&store, Dialect::OpenAiChat);
    let cache = runtime::ResponsesHistoryCache::new(2, 100_000);
    let mut request = client_request();
    request.store = Some(Some(false));
    let mut call = http_host::ready(
        generation::chat_responses::ResponsesViaChat::prepare_stream_with_history_cache(
            request,
            all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
            all_pairs::chat_response_context(),
            settings(),
            &state,
            &cache,
        ),
    )
    .unwrap();
    call.responses_websocket_output_on_lane(Some("main".into()))
        .unwrap();
    let feed = http_host::Feed::default();
    all_pairs::source(&feed, Dialect::OpenAiChat);
    feed.close();
    let host = http_host::Host::stream(store.clone(), feed);
    http_host::ready(call.start(&host, &(), &state)).unwrap();
    while let Some(chunk) = http_host::ready(call.next(&state)).unwrap() {
        if chunk.event.is_some() {
            let message: gproxy_protocol::wire::openai::responses::websocket::ServerMessage =
                serde_json::from_slice(&chunk.bytes).unwrap();
            assert_eq!(message.stream_id.as_deref(), Some("main"));
        }
    }
    assert!(
        !store
            .entries
            .lock()
            .unwrap()
            .keys()
            .any(|key| key.starts_with("responses-history:"))
    );
    let request = continuation(call.client_result().unwrap().clone());
    let mut next_target = all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat);
    next_target.identities = generation::GenerationIdentity::new(
        IdNamespace::with_bytes([93; 16]),
        IdNamespace::with_bytes([94; 16]),
        Dialect::OpenAi,
        Dialect::OpenAiChat,
    )
    .unwrap();
    let next = http_host::ready(
        generation::chat_responses::ResponsesViaChat::prepare_stream_with_history_cache(
            request.clone(),
            next_target,
            all_pairs::chat_response_context(),
            settings(),
            &state,
            &cache,
        ),
    )
    .unwrap();
    assert!(
        serde_json::to_string(next.target_request())
            .unwrap()
            .contains("answer")
    );
    let other_store = http_host::Store::default();
    let mut other_scope = access(&other_store, Dialect::OpenAiChat);
    other_scope.conversation_key = "elsewhere".into();
    assert!(
        http_host::ready(
            generation::chat_responses::ResponsesViaChat::prepare_stream_with_history_cache(
                request.clone(),
                all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
                all_pairs::chat_response_context(),
                settings(),
                &other_scope,
                &cache
            )
        )
        .is_err()
    );
    assert!(
        other_store.entries.lock().unwrap().is_empty(),
        "cache scope substitution must fail before new preparation writes"
    );
    drop(next);
    drop(call);
    drop(cache);
    let reconnected = runtime::ResponsesHistoryCache::new(2, 100_000);
    let error = http_host::ready(
        generation::chat_responses::ResponsesViaChat::prepare_stream_with_history_cache(
            request,
            all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
            all_pairs::chat_response_context(),
            settings(),
            &state,
            &reconnected,
        ),
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), TransformErrorKind::MissingState);
}
#[test]
fn ws_history_expiry_scope_and_missing_context_fail_before_send() {
    let store = Arc::new(http_host::Store::default());
    let state = access(&store, Dialect::OpenAiChat);
    let call = http_host::ready(
        generation::chat_responses::ResponsesViaChat::prepare_stream(
            client_request(),
            all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
            all_pairs::chat_response_context(),
            settings(),
            &state,
        ),
    )
    .unwrap();
    let request = continuation(run_client(call, store.clone(), Dialect::OpenAiChat));
    // Another origin is another host scope (core keys it by provider), which
    // this single-scope test store cannot express.
    for mode in ["conversation", "expiry", "absent"] {
        let mut changed = access(&store, Dialect::OpenAiChat);
        let mut req = request.clone();
        match mode {
            "conversation" => changed.conversation_key = "different".into(),
            "expiry" => {
                changed.now = changed.expires_at;
                changed.expires_at += Duration::from_secs(100);
            }
            "absent" => req.previous_response_id = Some(Some("resp_absent".into())),
            _ => unreachable!(),
        }
        assert!(
            http_host::ready(
                generation::chat_responses::ResponsesViaChat::prepare_stream(
                    req,
                    all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
                    all_pairs::chat_response_context(),
                    settings(),
                    &changed
                )
            )
            .is_err(),
            "{mode}"
        );
    }
}
#[test]
fn ws_history_canceled_applied_write_recovers_without_second_post_or_duplicate_cas() {
    let store = Arc::new(http_host::Store::default());
    let state = access(&store, Dialect::OpenAiChat);
    let mut call = http_host::ready(
        generation::chat_responses::ResponsesViaChat::prepare_stream(
            client_request(),
            all_pairs::target(Dialect::OpenAi, Dialect::OpenAiChat),
            all_pairs::chat_response_context(),
            settings(),
            &state,
        ),
    )
    .unwrap();
    let feed = http_host::Feed::default();
    all_pairs::source(&feed, Dialect::OpenAiChat);
    feed.close();
    let host = http_host::Host::stream(store.clone(), feed);
    call.responses_websocket_output().unwrap();
    http_host::ready(call.start(&host, &(), &state)).unwrap();
    *store.hang_key_prefix.lock().unwrap() = Some("responses-history:".into());
    loop {
        let mut next = Box::pin(call.next(&state));
        let polled = next.as_mut().poll(&mut Context::from_waker(Waker::noop()));
        if polled.is_pending() {
            assert!(store.hung.load(Ordering::SeqCst));
            break;
        }
        let Poll::Ready(Ok(Some(chunk))) = polled else {
            panic!("unexpected stream state")
        };
        assert!(
            !chunk
                .event
                .is_some_and(|event| matches!(event, StreamEvent::Completed(_)))
        );
    }
    assert!(call.client_result().is_none());
    let writes = store.serial.load(Ordering::SeqCst);
    while http_host::ready(call.next(&state)).unwrap().is_some() {}
    assert!(call.client_result().is_some());
    assert_eq!(store.serial.load(Ordering::SeqCst), writes);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn ws_connection_rejects_invocation_from_another_state_scope() {
    let connection_store = http_host::Store::default();
    let connection_state = access(&connection_store, Dialect::OpenAi);
    let other_store = http_host::Store::default();
    let mut other_state = access(&other_store, Dialect::OpenAi);
    other_state.conversation_key = "elsewhere".into();
    let host = Host::new(vec![frames_for(ws_body())]);
    let mut session = ws_open(&host, &connection_state);
    let mut call = http_host::ready(
        generation::chat_responses::ChatViaResponses::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::OpenAiChat)).unwrap(),
            all_pairs::target(Dialect::OpenAiChat, Dialect::OpenAi),
            settings(),
            &other_state,
        ),
    )
    .unwrap();
    assert!(http_host::ready(call.start_websocket(&mut session, &other_state)).is_err());
    assert_eq!(text_sends(&host.shared), 0);
    assert!(!session.native().is_poisoned());
}
