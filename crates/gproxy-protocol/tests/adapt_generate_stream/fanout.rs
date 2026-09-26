use super::*;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::generate::{
        fanout::*,
        stream::event::NativeEvent,
        stream::fanout::{FanoutBridge, FanoutEvent, FanoutStream},
    },
    capability::*,
    transform::generate::{claude_gemini as cg, gemini_responses::stream as gr},
};
use std::{collections::VecDeque, sync::Mutex};
struct MultiHost {
    host: Host,
    feeds: Mutex<VecDeque<Feed>>,
}
impl MultiHost {
    fn new(store: Arc<Store>, feeds: Vec<Feed>) -> Self {
        Self {
            host: Host::stream(store, Feed::default()),
            feeds: Mutex::new(feeds.into()),
        }
    }
}
impl Upstream for MultiHost {
    type Target = ();
    fn send<'a>(
        &'a self,
        target: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let feed = self
                .feeds
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected child POST");
            *self.host.response.lock().unwrap() = Some(WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::from_iter([(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("text/event-stream"),
                )]),
                body: HttpBody::Stream(Box::pin(feed)),
            });
            self.host.send(target, request).await
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected websocket")
    }
    fn limits(&self) -> CapabilityLimits {
        host::limits()
    }
}
fn access(store: &Store, native: Dialect) -> GenerationStateAccess<'_, Store> {
    let mut s = state(store);
    s.target = IdentityTarget::new("selected", native)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    s
}
fn target(client: Dialect, _native: Dialect) -> FanoutTarget {
    FanoutTarget {
        endpoint: Endpoint::new("/generation").unwrap(),

        options: FanoutOptions {
            response_policy: gproxy_protocol::transform::identity::TargetIdPolicy::new(client),
            namespace: IdNamespace([90; 16]),
            max_children: 2,
        },
    }
}
fn request(client: Dialect) -> Value {
    let mut v = all_pairs::request(client);
    if client == Dialect::OpenAiChat {
        v["n"] = json!(2);
    } else {
        v["generationConfig"]["candidateCount"] = json!(2);
    }
    v["foreign_sentinel"] = json!({"value":"must-not-leak"});
    v
}
fn gemini_context() -> GeminiViaClaudeStreamFacts {
    GeminiViaClaudeStreamFacts {
        max_tokens: Some(64),
        response: cg::stream::ClaudeToGeminiContext {
            usage: cg::ClaudeGeminiUsageFacts {
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                thinking_tokens: Some(0),
            },
        },
    }
}
fn check<B: FanoutBridge>(mut call: FanoutStream<B>, store: Arc<Store>)
where
    B::ClientEvent: FanoutEvent,
{
    let state = access(&store, B::NativeEvent::DIALECT);
    let first = Feed::default();
    let second = Feed::default();
    all_pairs::source(&first, B::NativeEvent::DIALECT);
    all_pairs::source(&second, B::NativeEvent::DIALECT);
    second.close();
    let host = MultiHost::new(store.clone(), vec![first.clone(), second]);
    let mut chunks = Vec::new();
    for _ in 0..8 {
        let chunk = ready(call.next(&host, &(), &state)).unwrap().unwrap();
        let value = serde_json::to_value(chunk.event.as_ref().unwrap()).unwrap();
        let response_id = value
            .get("id")
            .or_else(|| value.get("responseId"))
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(response_id, call.response_id());
        let record = ready(state.read(IdentityRole::Response, response_id))
            .unwrap()
            .unwrap();
        assert!(
            record.response_id.is_none(),
            "aggregate must not impersonate one native response"
        );
        assert!(value.get("usage").is_none_or(Value::is_null));
        assert!(value.get("usageMetadata").is_none());
        chunks.push(value.clone());
        if value.to_string().contains("answer") {
            break;
        }
    }
    assert!(
        chunks.iter().any(|v| v.to_string().contains("answer")),
        "first child content must precede its EOF"
    );
    assert_eq!(host.host.sent.lock().unwrap().len(), 1);
    assert!(call.client_result().is_none());
    first.close();
    while let Some(chunk) = ready(call.next(&host, &(), &state)).unwrap() {
        if let Some(e) = chunk.event {
            chunks.push(serde_json::to_value(e).unwrap());
        }
    }
    let body = serde_json::to_value(call.client_result().unwrap()).unwrap();
    let (choices, usage) = if B::ClientEvent::DIALECT == Dialect::OpenAiChat {
        (&body["choices"], &body["usage"])
    } else {
        (&body["candidates"], &body["usageMetadata"])
    };
    assert_eq!(choices.as_array().unwrap().len(), 2);
    let mut ids = Vec::new();
    for (index, c) in choices.as_array().unwrap().iter().enumerate() {
        assert_eq!(c["index"], json!(index));
        let id = if B::ClientEvent::DIALECT == Dialect::OpenAiChat {
            c["message"]["tool_calls"][0]["id"].as_str().unwrap()
        } else {
            c["content"]["parts"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|p| p.get("functionCall"))
                .unwrap()["id"]
                .as_str()
                .unwrap()
        };
        let saved = ready(state.read(IdentityRole::ToolCall, id))
            .unwrap()
            .unwrap();
        assert_eq!(saved.original_call_id.as_deref(), Some("tool:source"));
        assert_eq!(saved.tool_name.as_deref(), Some("f"));
        ids.push(id);
    }
    assert_eq!(ids[0], "tool:source");
    assert_ne!(ids[0], ids[1]);
    assert_eq!(
        usage
            .get("total_tokens")
            .or_else(|| usage.get("totalTokenCount")),
        Some(&json!(10))
    );
    let sent = host.host.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    for req in sent.iter() {
        let v: Value = serde_json::from_slice(&req.body).unwrap();
        assert!(!v.to_string().contains("sentinel"));
        assert_eq!(v["stream"], json!(true));
    }
    assert!(
        !store
            .entries
            .lock()
            .unwrap()
            .keys()
            .any(|k| k.starts_with("fanout-stream")),
        "fanout bookkeeping must stay in memory"
    );
}
#[test]
fn chat_claude_live_unique_ids_and_aggregate_usage() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::Claude);
    let call = ready(ChatViaClaudeFanout::prepare_stream(
        serde_json::from_value(request(Dialect::OpenAiChat)).unwrap(),
        target(Dialect::OpenAiChat, Dialect::Claude),
        {
            let mut contexts = vec![
                ClaudeToChatContext { created: 7 },
                ClaudeToChatContext { created: 9 },
            ]
            .into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &state,
    ))
    .unwrap();
    check(call, store);
}
#[test]
fn chat_responses_live_unique_ids_and_aggregate_usage() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let call = ready(ChatViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::OpenAiChat)).unwrap(),
        target(Dialect::OpenAiChat, Dialect::OpenAi),
        settings(),
        &state,
    ))
    .unwrap();
    check(call, store);
}
#[test]
fn gemini_claude_live_unique_ids_and_aggregate_usage() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::Claude);
    let call = ready(GeminiViaClaudeFanout::prepare_stream(
        serde_json::from_value(request(Dialect::Gemini)).unwrap(),
        target(Dialect::Gemini, Dialect::Claude),
        {
            let mut contexts = vec![gemini_context(), gemini_context()].into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &state,
    ))
    .unwrap();
    check(call, store);
}
#[test]
fn gemini_responses_live_unique_ids_and_aggregate_usage() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let call = ready(GeminiViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::Gemini)).unwrap(),
        target(Dialect::Gemini, Dialect::OpenAi),
        {
            let mut contexts = vec![
                gr::ResponsesToGeminiContext::default(),
                gr::ResponsesToGeminiContext::default(),
            ]
            .into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &state,
    ))
    .unwrap();
    check(call, store);
}
#[test]
fn fanout_scope_change_precedes_post() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let mut call = ready(ChatViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::OpenAiChat)).unwrap(),
        target(Dialect::OpenAiChat, Dialect::OpenAi),
        settings(),
        &state,
    ))
    .unwrap();
    let other = Store::default();
    let host = MultiHost::new(store, vec![]);
    assert!(ready(call.next(&host, &(), &access(&other, Dialect::OpenAi))).is_err());
    assert!(host.host.sent.lock().unwrap().is_empty());
}
#[test]
fn later_child_failure_keeps_first_receipt_without_false_success() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let mut call = ready(ChatViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::OpenAiChat)).unwrap(),
        target(Dialect::OpenAiChat, Dialect::OpenAi),
        settings(),
        &state,
    ))
    .unwrap();
    let first = Feed::default();
    all_pairs::source(&first, Dialect::OpenAi);
    first.close();
    let bad = Feed::default();
    bad.push("data: {\"type\":\"response.output_text.delta\"}\n\n");
    bad.close();
    let host = MultiHost::new(store.clone(), vec![first, bad]);
    let mut exposed = false;
    loop {
        match ready(call.next(&host, &(), &state)) {
            Ok(Some(_)) => exposed = true,
            Err(_) => break,
            Ok(None) => panic!("failed group must not complete"),
        }
    }
    assert!(exposed);
    assert!(call.children()[0].native_result().is_some());
    assert!(call.client_result().is_none());
    assert_eq!(host.host.sent.lock().unwrap().len(), 2);
    assert!(ready(call.next(&host, &(), &state)).is_err());
    assert_eq!(host.host.sent.lock().unwrap().len(), 2);
}
fn signed_context(id: &str) -> gr::ResponsesToGeminiContext {
    use gproxy_protocol::transform::{
        generate::gemini_responses::{GeminiReplayContext, RestoredGeminiPart},
        identity::{IdentityStateRecord, OpaqueField, OpaqueSignature},
    };
    let target = IdentityTarget::new("selected", Dialect::Gemini)
        .unwrap()
        .with_origin("original-gemini")
        .unwrap();
    let mut record = IdentityStateRecord::new(IdentityRole::ToolCall, target.clone());
    record.client_call_id = Some("tool:source".into());
    record.original_call_id = Some(id.into());
    record.tool_name = Some("f".into());
    record.opaque_signature = Some(
        OpaqueSignature::new(
            OpaqueField::GeminiPartThoughtSignature,
            "original-signature",
            "original-gemini",
            "selected",
        )
        .unwrap(),
    );
    gr::ResponsesToGeminiContext{restoration:GeminiReplayContext{target:Some(target),parts:std::collections::BTreeMap::from([("tool:source".into(),RestoredGeminiPart{state:record,part:serde_json::from_value(json!({"functionCall":{"id":id,"name":"f","args":{"x":1}},"thoughtSignature":"original-signature"})).unwrap()})]),image_files:Default::default()},..Default::default()}
}
#[test]
fn future_signed_id_is_reserved_before_first_unsigned_child_exposure() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let mut call = ready(GeminiViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::Gemini)).unwrap(),
        target(Dialect::Gemini, Dialect::OpenAi),
        {
            let mut contexts = vec![Default::default(), signed_context("tool:source")].into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &state,
    ))
    .unwrap();
    let feeds = (0..2)
        .map(|_| {
            let f = Feed::default();
            all_pairs::source(&f, Dialect::OpenAi);
            f.close();
            f
        })
        .collect();
    let host = MultiHost::new(store.clone(), feeds);
    let result = ready(call.collect(&host, &(), &state)).unwrap().value;
    let mut found = Vec::new();
    for candidate in result.candidates.unwrap() {
        let part = candidate
            .content
            .unwrap()
            .parts
            .unwrap()
            .into_iter()
            .find(|p| p.function_call.is_some())
            .unwrap();
        found.push((part.function_call.unwrap().id, part.thought_signature));
    }
    assert_ne!(found[0].0.as_deref(), Some("tool:source"));
    assert!(found[0].1.is_none());
    assert_eq!(
        found[1],
        (
            Some("tool:source".into()),
            Some("original-signature".into())
        )
    );
}
#[test]
fn duplicate_actual_signed_ids_stop_before_second_exposure() {
    let store = Arc::new(Store::default());
    let state = access(&store, Dialect::OpenAi);
    let mut call = ready(GeminiViaResponsesFanout::prepare_stream(
        serde_json::from_value(request(Dialect::Gemini)).unwrap(),
        target(Dialect::Gemini, Dialect::OpenAi),
        {
            let mut contexts = vec![signed_context("fixed"), signed_context("fixed")].into_iter();
            move |_| contexts.next().unwrap()
        },
        settings(),
        &state,
    ))
    .unwrap();
    let feeds = (0..2)
        .map(|_| {
            let f = Feed::default();
            all_pairs::source(&f, Dialect::OpenAi);
            f.close();
            f
        })
        .collect();
    let host = MultiHost::new(store.clone(), feeds);
    let mut exposed = 0;
    loop {
        match ready(call.next(&host, &(), &state)) {
            Ok(Some(chunk)) => {
                if let Some(event) = chunk.event {
                    exposed += event.tool_declarations(true).len();
                }
            }
            Err(error) => {
                assert_eq!(
                    error.kind(),
                    gproxy_protocol::transform::TransformErrorKind::InvalidResult
                );
                break;
            }
            Ok(None) => panic!("signed collision must fail"),
        }
    }
    assert_eq!(exposed, 1);
    assert!(call.client_result().is_none());
    assert_eq!(host.host.sent.lock().unwrap().len(), 2);
}

#[test]
fn chat_usage_opt_out_suppresses_wire_usage_but_retains_measured_aggregate() {
    fn run<
        B: FanoutBridge<
            ClientEvent = gproxy_protocol::wire::openai::chat::stream::ChatCompletionChunk,
        >,
    >(
        mut call: FanoutStream<B>,
        store: Arc<Store>,
    ) {
        let state = access(&store, B::NativeEvent::DIALECT);
        let feeds = (0..2)
            .map(|_| {
                let feed = Feed::default();
                all_pairs::source(&feed, B::NativeEvent::DIALECT);
                feed.close();
                feed
            })
            .collect();
        let host = MultiHost::new(store.clone(), feeds);
        while let Some(chunk) = ready(call.next(&host, &(), &state)).unwrap() {
            if let Some(event) = chunk.event {
                assert!(event.usage.flatten().is_none());
            }
        }
        assert_eq!(
            call.client_result()
                .unwrap()
                .usage
                .as_ref()
                .unwrap()
                .total_tokens,
            10
        );
        assert_eq!(host.host.sent.lock().unwrap().len(), 2);
    }
    for native in [Dialect::Claude, Dialect::OpenAi] {
        let store = Arc::new(Store::default());
        let state = access(&store, native);
        let mut input = request(Dialect::OpenAiChat);
        input["stream_options"]["include_usage"] = json!(false);
        match native {
            Dialect::Claude => run(
                ready(ChatViaClaudeFanout::prepare_stream(
                    serde_json::from_value(input).unwrap(),
                    target(Dialect::OpenAiChat, native),
                    {
                        let mut contexts = vec![
                            ClaudeToChatContext { created: 7 },
                            ClaudeToChatContext { created: 9 },
                        ]
                        .into_iter();
                        move |_| contexts.next().unwrap()
                    },
                    settings(),
                    &state,
                ))
                .unwrap(),
                store.clone(),
            ),
            Dialect::OpenAi => run(
                ready(ChatViaResponsesFanout::prepare_stream(
                    serde_json::from_value(input).unwrap(),
                    target(Dialect::OpenAiChat, native),
                    settings(),
                    &state,
                ))
                .unwrap(),
                store.clone(),
            ),
            _ => unreachable!(),
        }
    }
}

use super::host as http_host;
#[allow(dead_code)]
#[path = "../adapt_generate_image_resources/host.rs"]
mod image_host;
#[path = "fanout_images.rs"]
mod images;
