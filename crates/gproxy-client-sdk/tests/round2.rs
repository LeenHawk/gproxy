#![cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::*;
use protocol::{HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};
struct Reply {
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Vec<u8>,
}
impl Reply {
    fn json(body: Value) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: body.to_string().into_bytes(),
        }
    }
    fn sse(events: Vec<Value>) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: events
                .into_iter()
                .map(|event| format!("data: {event}\n\n"))
                .collect::<String>()
                .into_bytes(),
        }
    }
}
struct Script {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<Value>>,
}
impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}
impl Backend for Script {
    fn send(
        &self,
        _: OperationKey,
        _: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.requests
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes).unwrap());
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected retry or request");
            Ok(WireResponse {
                status: reply.status,
                headers: reply.headers,
                body: HttpBody::Bytes(reply.body.into()),
            })
        })
    }
}
fn answer() -> Value {
    let mut native = support::openai();
    native["id"] = json!("response_served");
    native["model"] = json!("served_model");
    native["created_at"] = json!(1770000000);
    native["output"] = json!([{"type":"message","id":"message_served","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]}]);
    native
}
fn early_events(native: &Value) -> Vec<Value> {
    let mut created = native.clone();
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created["usage"] = Value::Null;
    let mut events =
        vec![json!({"type":"response.created","sequence_number":0,"response":created})];
    events.extend(support::openai_events(native));
    events
}
#[test]
fn result_and_stream_expose_reported_identity_time_and_early_warnings() {
    futures_executor::block_on(async {
        let native = answer();
        let client = Client::new(Script::new(vec![
            Reply::json(native.clone()),
            Reply::sse(early_events(&native)),
        ]));
        let mut request = support::request(
            Dialect::Openai,
            vec![Message {
                role: Role::User,
                content: vec![Block::new(Content::Image(Image {
                    source: Asset::Url {
                        url: "https://example.com/image".into(),
                    },
                    media_type: None,
                    detail: Detail::Medium,
                }))],
            }],
        );
        let result = client.generate(&request).await.unwrap();
        assert_eq!(result.response.id.as_deref(), Some("response_served"));
        assert_eq!(result.response.model.as_deref(), Some("served_model"));
        assert_eq!(result.response.timestamp, Some(1770000000));
        assert_eq!(result.finish_reason.unified, FinishReasonKind::Stop);
        assert_eq!(result.finish_reason.raw.as_deref(), Some("completed"));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.field.ends_with(".detail") && w.kind == WarningKind::Compatibility)
        );
        let mut stream = client.stream(&request).await.unwrap();
        let first = stream.next().await.unwrap();
        assert!(matches!(&first,Event::Start {warnings} if warnings==&result.warnings));
        let second = stream.next().await.unwrap();
        assert!(matches!(&second,Event::ResponseMetadata {metadata} if metadata==&result.response));
        let mut events = vec![first, second];
        events.extend(stream.collect::<Vec<_>>().await);
        let aggregate = gproxy_client_sdk::aggregate(futures_util::stream::iter(events))
            .await
            .unwrap();
        assert_eq!(aggregate, result);
        request
            .provider_options
            .openai
            .responses
            .previous_response_id = result.response.id.map(Some);
        assert_eq!(
            dialects::openai::render(&request, false)
                .unwrap()
                .request
                .previous_response_id,
            Some(Some("response_served".into()))
        );
        let sent = client.backend.requests.lock().unwrap();
        assert_eq!(sent[0]["input"][0]["content"][0]["detail"], "auto");
        assert!(sent[0].get("warnings").is_none());
    });
}
#[test]
fn claude_and_gemini_metadata_do_not_invent_timestamps_or_requested_models() {
    let mut claude = support::claude();
    claude["model"] = json!("claude_served");
    let parsed = dialects::claude::parse(serde_json::from_value(claude.clone()).unwrap()).unwrap();
    assert_eq!(parsed.response.model.as_deref(), Some("claude_served"));
    assert_eq!(parsed.response.timestamp, None);
    assert_eq!(
        support::aggregate(support::decode(
            Dialect::Claude,
            support::claude_events(&claude)
        )),
        parsed
    );
    let first = json!({"modelVersion":"gemini_served","responseId":"gemini_response","candidates":[{"content":{"parts":[{"text":"hello"}]}}]});
    let last = json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}});
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let mut events = decoder.push_json(&first.to_string()).unwrap();
    events.extend(decoder.push_json(&last.to_string()).unwrap());
    events.extend(decoder.finish().unwrap());
    let parsed = support::aggregate(events);
    assert_eq!(parsed.response.id.as_deref(), Some("gemini_response"));
    assert_eq!(parsed.response.model.as_deref(), Some("gemini_served"));
    assert_eq!(parsed.response.timestamp, None);
    assert_eq!(parsed.finish_reason.raw.as_deref(), Some("STOP"));
}
#[test]
fn warnings_cover_detail_ttl_foreign_metadata_ignored_document_fields_and_legacy_controls() {
    let mut block = Block::new(Content::Document(Document {
        source: DocumentSource::Asset {
            source: Asset::Url {
                url: "https://example.com/doc.pdf".into(),
            },
        },
        media_type: Some("application/pdf".into()),
        title: None,
        context: Some("context".into()),
        citations: Some(true),
        detail: Detail::Original,
    }));
    block.cache = Some(CacheBreakpoint {
        ttl: Some(CacheTtl::FiveMinutes),
    });
    block.provider_metadata.insert(
        Vendor::Claude,
        serde_json::from_value(json!({"native":{"signature":"private"}})).unwrap(),
    );
    let request = support::request(
        Dialect::Openai,
        vec![Message {
            role: Role::User,
            content: vec![block],
        }],
    );
    let compiled = dialects::render(&request, false).unwrap();
    for field in [
        ".detail",
        ".cache.ttl",
        ".provider_metadata",
        ".context",
        ".citations",
    ] {
        assert!(
            compiled.warnings.iter().any(|w| w.field.ends_with(field)),
            "missing warning for {field}: {:?}",
            compiled.warnings
        );
    }
    assert!(!compiled.request.to_string().contains("private"));
    let mut claude = support::request(
        Dialect::Claude,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Url {
                    url: "https://example.com/image".into(),
                },
                media_type: None,
                detail: Detail::High,
            }))],
        }],
    );
    claude.settings.temperature = Some(0.3);
    claude.settings.speech = Some(SpeechSettings {
        voice: "voice".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let warnings = dialects::claude::render(&claude, false).unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w.kind == WarningKind::Deprecated && w.field == "settings.temperature")
    );
    assert!(warnings.iter().any(|w| w.field == "settings.speech"));
    assert!(warnings.iter().any(|w| w.field.ends_with(".detail")));
    let mut gemini = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::User,
            content: vec![Block::text("cache")],
        }],
    );
    gemini.messages[0].content[0].cache = Some(CacheBreakpoint { ttl: None });
    assert!(
        dialects::gemini::render(&gemini)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".cache"))
    );
}
#[test]
fn native_options_override_portable_controls_with_diagnostics() {
    let mut request = support::request(Dialect::Gemini, vec![]);
    request.settings.temperature = Some(0.8);
    request.settings.max_output_tokens = Some(90);
    request.settings.output_modalities = vec![Modality::Audio];
    request.provider_options.gemini.generation_config = Some(
        serde_json::from_value(
            json!({"temperature":0.2,"maxOutputTokens":80,"responseModalities":["TEXT"]}),
        )
        .unwrap(),
    );
    let compiled = dialects::gemini::render(&request).unwrap();
    assert_eq!(
        compiled
            .request
            .generation_config
            .as_ref()
            .unwrap()
            .temperature,
        Some(0.2)
    );
    assert_eq!(
        compiled
            .request
            .generation_config
            .as_ref()
            .unwrap()
            .max_output_tokens,
        Some(80)
    );
    for field in [
        "settings.temperature",
        "settings.max_output_tokens",
        "settings.output_modalities",
    ] {
        assert!(compiled.warnings.iter().any(|w| w.field == field));
    }
}
#[test]
fn reasoning_maps_levels_budgets_defaults_and_vendor_overrides() {
    for (effort, wire) in [
        (ReasoningEffort::None, "none"),
        (ReasoningEffort::Minimal, "minimal"),
        (ReasoningEffort::Low, "low"),
        (ReasoningEffort::Medium, "medium"),
        (ReasoningEffort::High, "high"),
        (ReasoningEffort::Xhigh, "xhigh"),
        (ReasoningEffort::Max, "max"),
    ] {
        for dialect in [Dialect::Openai, Dialect::OpenaiChat] {
            let mut request = support::request(dialect, vec![]);
            request.settings.reasoning = Some(ReasoningSettings {
                effort: effort.clone(),
                token_budget: None,
            });
            let native = dialects::render(&request, false).unwrap().request;
            assert_eq!(
                if dialect == Dialect::Openai {
                    &native["reasoning"]["effort"]
                } else {
                    &native["reasoning_effort"]
                },
                wire
            );
        }
    }
    let mut claude = support::request(Dialect::Claude, vec![]);
    claude.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::Minimal,
        token_budget: None,
    });
    let compiled = dialects::render(&claude, false).unwrap();
    assert_eq!(compiled.request["thinking"]["type"], "adaptive");
    assert_eq!(compiled.request["output_config"]["effort"], "low");
    assert!(!compiled.warnings.is_empty());
    claude.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::ProviderDefault,
        token_budget: Some(2048),
    });
    let native = dialects::render(&claude, false).unwrap().request;
    assert_eq!(native["thinking"]["budget_tokens"], 2048);
    claude.provider_options.claude.thinking =
        Some(serde_json::from_value(json!({"type":"disabled"})).unwrap());
    let compiled = dialects::render(&claude, false).unwrap();
    assert_eq!(compiled.request["thinking"]["type"], "disabled");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning")
    );
    let mut gemini = support::request(Dialect::Gemini, vec![]);
    gemini.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::Max,
        token_budget: None,
    });
    let compiled = dialects::render(&gemini, false).unwrap();
    assert_eq!(
        compiled.request["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "HIGH"
    );
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.effort")
    );
    gemini.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::None,
        token_budget: None,
    });
    assert_eq!(
        dialects::render(&gemini, false).unwrap().request["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        0
    );
    let mut openai = support::request(Dialect::Openai, vec![]);
    openai.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::High,
        token_budget: Some(12),
    });
    openai.provider_options.openai.responses.reasoning = Some(Some(
        serde_json::from_value(json!({"effort":"low"})).unwrap(),
    ));
    let compiled = dialects::render(&openai, false).unwrap();
    assert_eq!(compiled.request["reasoning"]["effort"], "low");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.token_budget")
    );
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field == "settings.reasoning.effort")
    );
    openai.settings.reasoning = Some(ReasoningSettings {
        effort: ReasoningEffort::ProviderDefault,
        token_budget: None,
    });
    openai.provider_options = ProviderOptions::default();
    assert!(
        dialects::render(&openai, false)
            .unwrap()
            .request
            .get("reasoning")
            .is_none()
    );
}
#[test]
fn refusal_is_portable_and_claude_stream_block_end_is_authoritative() {
    let mut native = answer();
    native["output"][0]["content"] = json!([{"type":"refusal","refusal":"Cannot help"}]);
    let parsed = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert!(matches!(
        &parsed.content[0].content,
        Content::Text(Text { refusal: true, .. })
    ));
    let request = support::request(Dialect::OpenaiChat, support::persisted(parsed));
    let compiled = dialects::render(&request, false).unwrap();
    assert_eq!(compiled.request["messages"][0]["refusal"], "Cannot help");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".provider_metadata"))
    );
    let mut claude = support::claude();
    claude["content"] = json!([{"type":"text","text":"Cannot help"}]);
    claude["stop_reason"] = json!("refusal");
    let buffered =
        dialects::claude::parse(serde_json::from_value(claude.clone()).unwrap()).unwrap();
    let events = support::decode(Dialect::Claude, support::claude_events(&claude));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::BlockEnd {
            block: Block {
                content: Content::Text(Text { refusal: true, .. }),
                ..
            },
            ..
        }
    )));
    assert_eq!(support::aggregate(events), buffered);
    let switched = support::request(Dialect::Gemini, support::persisted(buffered));
    let compiled = dialects::render(&switched, false).unwrap();
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".refusal"))
    );
    assert!(matches!(
        switched.messages[0].content[0].content,
        Content::Text(Text { refusal: true, .. })
    ));
}
#[test]
fn retrieved_sources_do_not_require_inline_anchors() {
    let native = json!({"candidates":[{"content":{"parts":[{"text":"answer"}]},"finishReason":"STOP","groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://example.com","title":"Web"}},{"retrievedContext":{"title":"Unlocated","text":"snippet"}},{"retrievedContext":{"uri":"https://example.com/doc","title":"Doc","text":"document text","pageNumber":1}}]}}]});
    let parsed = dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(parsed.sources.len(), 3);
    assert!(parsed.sources[1].source.is_none());
    assert_eq!(parsed.sources[1].text.as_deref(), Some("snippet"));
    assert!(
        matches!(&parsed.content[0].content,Content::Text(Text {citations,..}) if citations.is_empty())
    );
    let events = support::decode(Dialect::Gemini, vec![native]);
    assert_eq!(support::aggregate(events), parsed);
    let mut claude = support::claude();
    claude["content"] = json!([{"type":"web_search_tool_result","tool_use_id":"search","content":[{"type":"web_search_result","encrypted_content":"cipher","title":"Retrieved","url":"https://example.com"}]}]);
    assert_eq!(
        dialects::claude::parse(serde_json::from_value(claude).unwrap())
            .unwrap()
            .sources
            .len(),
        1
    );
    let mut openai = answer();
    openai["output"] = json!([{"type":"web_search_call","id":"s1","status":"completed","action":{"type":"search","query":"q","sources":[{"type":"url","url":"https://example.com"}]}},{"type":"file_search_call","id":"f1","status":"completed","queries":["q"],"results":[{"text":"unlocated file snippet"}]}]);
    let parsed = dialects::openai::parse(serde_json::from_value(openai.clone()).unwrap()).unwrap();
    assert_eq!(parsed.sources.len(), 2);
    assert_eq!(
        parsed.sources[1].text.as_deref(),
        Some("unlocated file snippet")
    );
    assert_eq!(
        support::aggregate(support::decode(
            Dialect::Openai,
            support::openai_events(&openai)
        )),
        parsed
    );
}
#[test]
fn finish_preserves_raw_incomplete_values_and_partial_stream_content() {
    let mut native = answer();
    native["status"] = json!("incomplete");
    native["incomplete_details"] = json!({"reason":"max_output_tokens"});
    let result = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert_eq!(result.finish_reason.unified, FinishReasonKind::Length);
    assert_eq!(
        result.finish_reason.raw.as_deref(),
        Some("max_output_tokens")
    );
    let mut decoder = stream::StreamDecoder::new(Dialect::OpenaiChat);
    let mut events=decoder.push_json(&json!({"id":"partial","object":"chat.completion.chunk","created":1,"model":"served","choices":[{"index":0,"delta":{"content":"partial answer"},"finish_reason":null}]}).to_string()).unwrap();
    events.extend(decoder.finish().unwrap());
    let partial = support::aggregate(events);
    assert_eq!(partial.finish_reason.unified, FinishReasonKind::Interrupted);
    assert_eq!(partial.finish_reason.raw, None);
    assert!(
        matches!(&partial.content[0].content,Content::Text(Text {text,..}) if text=="partial answer")
    );
    let mut decoder = stream::StreamDecoder::new(Dialect::Openai);
    let mut failed = answer();
    failed["status"] = json!("failed");
    failed["error"] = json!({"code":"server_error","message":"busy"});
    let events = decoder
        .push_json(
            &json!({"type":"response.failed","sequence_number":1,"response":failed}).to_string(),
        )
        .unwrap();
    assert!(events.iter().any(|event|matches!(event,Event::Finish {reason:FinishReason {unified:FinishReasonKind::Error,raw:Some(raw)},..} if raw=="failed")));
}
#[test]
fn error_classes_keep_transport_facts_and_do_not_retry_in_band_failures() {
    for (status, kind, retryable) in [
        (402, ErrorKind::PaymentRequired, false),
        (404, ErrorKind::NotFound, false),
        (408, ErrorKind::Timeout, true),
        (409, ErrorKind::Upstream, true),
        (429, ErrorKind::RateLimit, true),
        (503, ErrorKind::Upstream, true),
        (529, ErrorKind::Overloaded, true),
    ] {
        let headers = http::HeaderMap::from_iter([(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("Wed, 01 Jan 2031 00:00:00 GMT"),
        )]);
        let error = Error::from_response(status, &headers, "original body".into());
        assert_eq!(error.kind, kind);
        assert_eq!(error.status, Some(status));
        assert_eq!(error.retryable, retryable);
        assert_eq!(error.body.as_deref(), Some("original body"));
        assert_eq!(
            error.retry_after.as_deref(),
            Some("Wed, 01 Jan 2031 00:00:00 GMT")
        );
    }
    let context = Error::from_response(
        400,
        &http::HeaderMap::new(),
        json!({"error":{"type":"api_error","code":"context_length_exceeded"}}).to_string(),
    );
    assert_eq!(context.kind, ErrorKind::ContextTooLong);
    assert!(!context.retryable);
    let overloaded = Error::from_event(
        json!({"error":{"type":"overloaded_error","message":"busy"}}).to_string(),
    );
    assert_eq!(overloaded.kind, ErrorKind::Overloaded);
    assert!(overloaded.retryable);
    assert_eq!(overloaded.status, None);
    futures_executor::block_on(async {
        let event =
            json!({"error":{"code":402,"message":"credits"},"choices":[{"finish_reason":"error"}]});
        let mut reply = Reply::sse(vec![event.clone()]);
        reply.status = http::StatusCode::CREATED;
        reply.headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("9"),
        );
        let client = Client::new(Script::new(vec![reply]));
        let request = support::request(Dialect::OpenaiChat, vec![]);
        let events = client
            .stream(&request)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        let error = events
            .iter()
            .find_map(|event| match event {
                Event::Error { error } => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.kind, ErrorKind::PaymentRequired);
        assert_eq!(error.status, Some(201));
        assert_eq!(error.body.as_deref(), Some(event.to_string().as_str()));
        assert_eq!(error.retry_after.as_deref(), Some("9"));
        assert!(!error.retryable);
        assert!(events.iter().any(|event|matches!(event,Event::Finish {reason:FinishReason {unified:FinishReasonKind::Error,raw:Some(raw)},..} if raw=="error")));
        assert_eq!(client.backend.requests.lock().unwrap().len(), 1);
    });
}

#[test]
fn malformed_native_records_keep_the_original_data_and_finish_interrupted() {
    futures_executor::block_on(async {
        let malformed = json!({"not":"a native response record"});
        let client = Client::new(Script::new(vec![Reply::sse(vec![malformed.clone()])]));
        let events = client
            .stream(&support::request(Dialect::OpenaiChat, vec![]))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        let error = events
            .iter()
            .find_map(|event| match event {
                Event::Error { error } => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(error.status, Some(200));
        assert_eq!(error.body.as_deref(), Some(malformed.to_string().as_str()));
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Finish {
                reason: FinishReason {
                    unified: FinishReasonKind::Interrupted,
                    ..
                },
                ..
            }
        )));
    });
}
#[test]
fn typed_transport_timeouts_are_retryable_and_do_not_synthesize_completion() {
    struct TimeoutTransport;
    impl Backend for TimeoutTransport {
        fn send(
            &self,
            _: OperationKey,
            _: Option<&str>,
            _: WireRequest<HttpBody>,
        ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
            Box::pin(async {
                let event = json!({"id":"partial","object":"chat.completion.chunk","created":1,"model":"served","choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":null}]});
                let frames = vec![
                    Ok(format!("data: {event}\n\n").into()),
                    Err(
                        Box::new(std::io::Error::new(std::io::ErrorKind::TimedOut, "stalled"))
                            as protocol::connection::TransportError,
                    ),
                ];
                Ok(WireResponse {
                    status: http::StatusCode::OK,
                    headers: http::HeaderMap::new(),
                    body: HttpBody::Stream(Box::pin(futures_util::stream::iter(frames))),
                })
            })
        }
    }
    futures_executor::block_on(async {
        let events = Client::new(TimeoutTransport)
            .stream(&support::request(Dialect::OpenaiChat, vec![]))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        let error = events
            .iter()
            .find_map(|event| match event {
                Event::Error { error } => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Timeout);
        assert_eq!(error.status, Some(200));
        assert!(error.retryable);
        assert!(
            events
                .iter()
                .any(|event| matches!(event,Event::TextDelta {delta,..} if delta=="partial"))
        );
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Finish {
                reason: FinishReason {
                    unified: FinishReasonKind::Interrupted,
                    ..
                },
                ..
            }
        )));
    });
}

#[test]
fn successful_http_replies_with_native_errors_keep_the_whole_observed_body() {
    futures_executor::block_on(async {
        let mut failed = answer();
        failed["status"] = json!("failed");
        failed["error"] = json!({"code":"server_error","message":"busy"});
        failed["audit_extra"] = json!({"preserved":true});
        let event = json!({"type":"response.failed","sequence_number":1,"response":failed});
        let client = Client::new(Script::new(vec![
            Reply::json(failed.clone()),
            Reply::sse(vec![event.clone()]),
        ]));
        let request = support::request(Dialect::Openai, vec![]);
        let error = client.generate(&request).await.unwrap_err();
        assert_eq!(error.status, Some(200));
        assert_eq!(error.kind, ErrorKind::Upstream);
        assert!(error.retryable);
        assert_eq!(error.body.as_deref(), Some(failed.to_string().as_str()));
        let events = client
            .stream(&request)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        let error = events
            .iter()
            .find_map(|event| match event {
                Event::Error { error } => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.status, Some(200));
        assert_eq!(error.body.as_deref(), Some(event.to_string().as_str()));
    });
}
