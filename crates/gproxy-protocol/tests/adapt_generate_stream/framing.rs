use super::*;
use gproxy_protocol::{
    HttpBody, WireResponse,
    adapt::generate::{chat_gemini::ChatViaGemini, claude_gemini::GeminiViaClaude},
    transform::generate::{self as p, stream::gemini::GeminiStreamCollector},
    wire::gemini as g,
};
#[test]
fn actual_gemini_array_and_ndjson_sources_preserve_live_output_and_final_tools() {
    for framing in [SourceFraming::JsonArray, SourceFraming::Ndjson] {
        let store = Arc::new(Store::default());
        let access = all_pairs::access(&store, Dialect::Gemini);
        let feed = Feed::default();
        let json = all_pairs::native_response(Dialect::Gemini).to_string();
        feed.push(match framing {
            SourceFraming::JsonArray => format!("[{json}]"),
            _ => format!("{json}\n"),
        });
        let host = Host::stream(store.clone(), feed.clone());
        host.response
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .headers
            .insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(match framing {
                    SourceFraming::JsonArray => "application/json",
                    _ => "application/x-ndjson",
                }),
            );
        let mut config = settings();
        config.source_framing = framing;
        let mut call = ready(ChatViaGemini::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::OpenAiChat)).unwrap(),
            all_pairs::target(Dialect::OpenAiChat, Dialect::Gemini),
            ChatViaGeminiStreamFacts {
                function_names: Default::default(),
                response: p::gemini_chat::stream::GeminiToChatContext {
                    created: 7,
                    model: Some("selected".into()),
                },
            },
            config,
            &access,
        ))
        .unwrap();
        ready(call.start(&host, &(), &access)).unwrap();
        let first = ready(call.next(&access)).unwrap().unwrap();
        assert!(String::from_utf8_lossy(&first.bytes).contains("answer"));
        assert!(call.client_result().is_none());
        feed.close();
        let GenerationOutcome::Success { response, .. } = ready(call.collect(&access)).unwrap()
        else {
            panic!("rejected")
        };
        assert_eq!(
            response.headers[http::header::CONTENT_TYPE],
            "application/json"
        );
        assert_eq!(
            response.body.choices[0]
                .message
                .tool_calls
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }
}
#[test]
fn actual_gemini_array_and_ndjson_outputs_recollect_to_exact_client_result() {
    for framing in [SourceFraming::JsonArray, SourceFraming::Ndjson] {
        let store = Arc::new(Store::default());
        let access = state(&store);
        let feed = Feed::default();
        all_pairs::source(&feed, Dialect::Claude);
        feed.close();
        let host = Host::stream(store.clone(), feed);
        let mut config = settings();
        config.client_framing = framing;
        let mut call = ready(GeminiViaClaude::prepare_stream(
            serde_json::from_value(all_pairs::request(Dialect::Gemini)).unwrap(),
            all_pairs::target(Dialect::Gemini, Dialect::Claude),
            GeminiViaClaudeStreamFacts {
                max_tokens: Some(64),
                response: p::claude_gemini::stream::ClaudeToGeminiContext {
                    usage: p::claude_gemini::ClaudeGeminiUsageFacts {
                        cache_creation_input_tokens: Some(0),
                        cache_read_input_tokens: Some(0),
                        thinking_tokens: Some(0),
                    },
                },
            },
            config,
            &access,
        ))
        .unwrap();
        ready(call.start(&host, &(), &access)).unwrap();
        let mut bytes = Vec::new();
        while let Some(chunk) = ready(call.next(&access)).unwrap() {
            bytes.extend_from_slice(&chunk.bytes);
        }
        let events: Vec<g::GenerateContentResponseBody> = match framing {
            SourceFraming::JsonArray => serde_json::from_slice(&bytes).unwrap(),
            _ => String::from_utf8(bytes)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect(),
        };
        let mut collector = GeminiStreamCollector::new(Default::default());
        for event in events {
            collector.push(event).unwrap();
        }
        assert_eq!(
            &collector.finish().unwrap().value,
            call.client_result().unwrap()
        );
    }
}
#[test]
fn final_cas_cancellation_keeps_client_result_hidden_until_durable_acknowledgment() {
    let store = Arc::new(Store::default());
    let access = state(&store);
    let feed = Feed::default();
    prefix(&feed);
    let host = Host::stream(store.clone(), feed.clone());
    let mut call = prepared(&store, true);
    ready(call.start(&host, &(), &access)).unwrap();
    loop {
        if String::from_utf8_lossy(&ready(call.next(&access)).unwrap().unwrap().bytes)
            .contains("hello")
        {
            break;
        }
    }
    terminal(&feed);
    store.hang_applied.store(true, Ordering::SeqCst);
    {
        let mut next = Box::pin(call.next(&access));
        for _ in 0..10000 {
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            if store.hung.load(Ordering::SeqCst) {
                break;
            }
        }
        assert!(store.hung.load(Ordering::SeqCst));
    }
    assert!(call.native_result().is_some());
    assert!(call.client_result().is_none());
    let writes = store.serial.load(Ordering::SeqCst);
    let reads = feed.polls();
    assert!(
        ready(call.next(&access))
            .unwrap()
            .unwrap()
            .event
            .unwrap()
            .choices[0]
            .finish_reason
            .is_some()
    );
    assert_eq!(store.serial.load(Ordering::SeqCst), writes);
    assert_eq!(feed.polls(), reads);
    assert!(call.client_result().is_some());
}
#[test]
fn non_2xx_and_native_provider_errors_remain_inspectable_without_success_framing() {
    let store = Arc::new(Store::default());
    let access = state(&store);
    let host = Host::stream(store.clone(), Feed::default());
    let raw = bytes::Bytes::from_static(b"{\"error\":\"denied\"}");
    *host.response.lock().unwrap() = Some(WireResponse {
        status: http::StatusCode::FORBIDDEN,
        headers: http::HeaderMap::new(),
        body: HttpBody::Bytes(raw.clone()),
    });
    let mut call = prepared(&store, true);
    let StreamStart::Rejected(rejected) = ready(call.start(&host, &(), &access)).unwrap() else {
        panic!("false success")
    };
    assert_eq!(rejected.body, raw);
    assert_eq!(call.rejected_response().unwrap().body, raw);
    assert!(matches!(
        ready(call.collect(&access)).unwrap(),
        GenerationOutcome::Rejected(_)
    ));
    let store = Arc::new(Store::default());
    let access = state(&store);
    let feed = Feed::default();
    prefix(&feed);
    event(
        &feed,
        json!({"type":"error","error":{"type":"overloaded_error","message":"provider busy"}}),
    );
    feed.close();
    let host = Host::stream(store.clone(), feed);
    let mut call = prepared(&store, true);
    ready(call.start(&host, &(), &access)).unwrap();
    let mut bytes = Vec::new();
    loop {
        match ready(call.next(&access)) {
            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk.bytes),
            Err(_) => break,
            Ok(None) => panic!("provider error became success"),
        }
    }
    assert!(!String::from_utf8_lossy(&bytes).contains("[DONE]"));
    assert!(
        serde_json::to_string(call.last_native_event().unwrap())
            .unwrap()
            .contains("provider busy")
    );
    assert_eq!(call.native_metadata().unwrap().status, http::StatusCode::OK);
    assert!(call.client_result().is_none());
}
#[test]
fn nonstreaming_client_collects_actual_usage_from_streaming_upstream() {
    let store = Arc::new(Store::default());
    let access = state(&store);
    let feed = Feed::default();
    prefix(&feed);
    terminal(&feed);
    let host = Host::stream(store.clone(), feed);
    let mut input = request(false);
    input.stream = Some(Some(false));
    let mut call = ready(ChatViaClaude::prepare_stream(
        input,
        selected(),
        ClaudeToChatContext { created: 7 },
        settings(),
        &access,
    ))
    .unwrap();
    ready(call.start(&host, &(), &access)).unwrap();
    let GenerationOutcome::Success { response, .. } = ready(call.collect(&access)).unwrap() else {
        panic!("rejected")
    };
    assert_eq!(response.body.usage.unwrap().total_tokens, 5);
    assert_eq!(
        response.body.choices[0].message.content.as_deref(),
        Some("hello")
    );
}

#[test]
fn rejected_native_lifecycle_releases_live_body_before_invocation_is_dropped() {
    struct Tracked {
        feed: Feed,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }
    impl futures_core::Stream for Tracked {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::pin::Pin::new(&mut self.get_mut().feed).poll_next(cx)
        }
    }
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }
    let store = Arc::new(Store::default());
    let access = state(&store);
    let feed = Feed::default();
    event(&feed, json!({"type":"content_block_stop","index":0}));
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let host = Host::stream(store.clone(), feed.clone());
    host.response.lock().unwrap().as_mut().unwrap().body = HttpBody::Stream(Box::pin(Tracked {
        feed,
        dropped: dropped.clone(),
    }));
    let mut call = prepared(&store, true);
    ready(call.start(&host, &(), &access)).unwrap();
    assert!(ready(call.next(&access)).is_err());
    assert!(
        dropped.load(Ordering::SeqCst),
        "failed native lifecycle retained a live upstream stream"
    );
    assert!(ready(call.next(&access)).is_err());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn stream_preparation_cannot_move_to_another_state_authority_before_start() {
    let original = Arc::new(Store::default());
    let replacement = Arc::new(Store::default());
    // The invocation tells state apart by what it was prepared with: the
    // target, conversation and expiry its records are keyed and aged by.
    let moved = |store| {
        let mut access = state(store);
        access.conversation_key = "elsewhere".into();
        access
    };
    let mut call = prepared(&original, true);
    let host = Host::stream(replacement.clone(), Feed::default());
    let result = ready(call.start(&host, &(), &moved(&replacement)));
    assert!(
        result.is_err(),
        "prepared invocation silently changed its state authority"
    );
    assert!(host.sent.lock().unwrap().is_empty());
    let feed = Feed::default();
    prefix(&feed);
    let original_host = Host::stream(original.clone(), feed.clone());
    ready(call.start(&original_host, &(), &state(&original))).unwrap();
    assert!(ready(call.next(&moved(&replacement))).is_err());
    assert_eq!(feed.polls(), 0);
    assert!(replacement.entries.lock().unwrap().is_empty());
    assert!(ready(call.next(&state(&original))).unwrap().is_some());
    // Mid-stream, another authority is still refused before anything is read.
    let reads = feed.polls();
    assert!(ready(call.next(&moved(&replacement))).is_err());
    assert_eq!(feed.polls(), reads);
    assert!(replacement.entries.lock().unwrap().is_empty());
}
