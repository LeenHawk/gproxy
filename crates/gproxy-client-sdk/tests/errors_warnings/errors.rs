use super::*;

#[cfg(feature = "gemini")]
#[test]
fn nameless_results_without_an_earlier_call_return_invalid_data() {
    let block = Block::new(Content::ToolResult(ToolResult {
        id: "missing".into(),
        name: String::new(),
        output: ToolOutput::Json(json!("result")),
        provider_executed: false,
        is_error: false,
    }));
    let request = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::Tool,
            content: vec![block],
        }],
    );
    assert!(matches!(
        dialects::gemini::render(&request),
        Err(Error {
            kind: ErrorKind::InvalidData,
            ..
        })
    ));
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn malformed_metadata_and_options_return_errors_without_panics() {
    for dialect in [Dialect::Claude, Dialect::Openai, Dialect::Gemini] {
        let mut broken = Block::text("hello");
        broken.provider_metadata.insert(
            dialect.vendor(),
            serde_json::from_value(json!({"native":"not an object"})).unwrap(),
        );
        let request = support::request(
            dialect,
            vec![Message {
                role: Role::Assistant,
                content: vec![broken],
            }],
        );
        assert!(matches!(
            dialects::render(&request, false),
            Err(Error {
                kind: ErrorKind::InvalidData,
                ..
            })
        ));
        for native in [
            json!("not an object"),
            json!({"type":123}),
            json!({"thoughtSignature":[]} ),
        ] {
            let mut block = Block::text("hello");
            block.provider_metadata.insert(
                dialect.vendor(),
                serde_json::from_value(json!({"native":native})).unwrap(),
            );
            let request = support::request(
                dialect,
                vec![Message {
                    role: Role::Assistant,
                    content: vec![block],
                }],
            );
            // Malformed typed fields are rejected. Unknown extensions remain
            // valid metadata; neither case may panic.
            let _ = dialects::render(&request, false);
        }
        let invalid = json!("invalid object");
        let rejected = match dialect {
            Dialect::Claude => {
                serde_json::from_value::<options::ClaudeOptions>(json!({"output_config":invalid}))
                    .is_err()
            }
            Dialect::Openai => {
                serde_json::from_value::<options::ResponsesOptions>(json!({"text":invalid}))
                    .is_err()
            }
            Dialect::Gemini => serde_json::from_value::<options::GeminiOptions>(
                json!({"generationConfig":invalid}),
            )
            .is_err(),
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => {
                serde_json::from_value::<options::ChatOptions>(json!({"audio":invalid})).is_err()
            }
        };
        assert!(rejected);
    }
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
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

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
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

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
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

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
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
