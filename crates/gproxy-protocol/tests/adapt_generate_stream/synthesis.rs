use super::*;
use gproxy_protocol::{
    HttpBody, WireResponse,
    adapt::generate::stream::{
        event::NativeEvent,
        reader::{NativeFrame, NativeReader},
        synthesize::{self, CompleteResponse, GenerationStreamOutcome},
    },
    transform::{
        Report,
        generate::claude_chat,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{claude::generate_content as c, gemini as g, openai::responses as r},
};
fn roundtrip<C: CompleteResponse>(body: C) {
    let expected = serde_json::to_value(&body).unwrap();
    let outcome = GenerationOutcome::Success {
        response: WireResponse {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::from_iter([(
                http::header::CONTENT_LENGTH,
                http::HeaderValue::from_static("1"),
            )]),
            body,
        },
        report: Report::default(),
    };
    let GenerationStreamOutcome::Success { response, .. } = synthesize::synthesize(
        outcome,
        IdNamespace([103; 16]),
        SourceFraming::Sse,
        settings().codec,
        settings().events,
    )
    .unwrap() else {
        panic!("rejected")
    };
    assert_eq!(
        response.headers[http::header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert!(!response.headers.contains_key(http::header::CONTENT_LENGTH));
    let mut reader = NativeReader::new(response.body, SourceFraming::Sse, settings().codec, 1024);
    let mut collector = C::Event::collector(
        IdentityFlow::new(IdNamespace([104; 16])),
        TargetIdPolicy::new(C::Event::DIALECT),
        settings().events,
    );
    while let Some(frame) = ready(reader.next::<C::Event>()).unwrap() {
        match frame {
            NativeFrame::Event { value, .. } => {
                C::Event::collect(&mut collector, value).unwrap();
            }
            NativeFrame::Done => C::Event::collect_done(&mut collector).unwrap(),
        }
    }
    let actual = C::Event::collected(collector).unwrap().value.value;
    assert_eq!(serde_json::to_value(actual).unwrap(), expected);
}
#[test]
fn all_four_complete_wire_results_synthesize_native_lifecycles_without_identity_changes() {
    roundtrip::<h::GenerateContentResponseBody>(
        serde_json::from_value(all_pairs::native_response(Dialect::OpenAiChat)).unwrap(),
    );
    roundtrip::<c::GenerateContentResponseBody>(
        serde_json::from_value(all_pairs::native_response(Dialect::Claude)).unwrap(),
    );
    roundtrip::<g::GenerateContentResponseBody>(
        serde_json::from_value(all_pairs::native_response(Dialect::Gemini)).unwrap(),
    );
    roundtrip::<r::GenerateContentResponseBody>(
        serde_json::from_value(all_pairs::native_response(Dialect::OpenAi)).unwrap(),
    );
}
#[test]
fn buffered_post_state_is_complete_before_synthesis_exposes_any_bytes() {
    for include_usage in [false, true] {
        buffered_post_synthesis(include_usage);
    }
}
fn buffered_post_synthesis(include_usage: bool) {
    let store = Arc::new(Store::default());
    let access = state(&store);
    let host = Host::stream(store.clone(), Feed::default());
    *host.response.lock().unwrap() = Some(WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Bytes(
            all_pairs::native_response(Dialect::Claude)
                .to_string()
                .into(),
        ),
    });
    let input = request(include_usage);
    let selected = selected();
    let mut prepared = ready(ChatViaClaude::prepare_for_stream_synthesis(
        input,
        selected.endpoint,
        selected.identities,
        &access,
    ))
    .unwrap();
    assert_eq!(prepared.original_request().stream, Some(Some(true)));
    assert_eq!(prepared.target_request().stream, Some(false));
    let mut progress = GenerationProgress::default();
    let result =
        ready(
            prepared.invoke(&host, &(), settings().codec, &access, &mut progress, |_| {
                Ok(claude_chat::ResponseSupplement {
                    created_unix_seconds: Some(7),
                })
            }),
        )
        .unwrap();
    let writes = store.serial.load(Ordering::SeqCst);
    let GenerationStreamOutcome::Success { response, .. } = synthesize::synthesize_chat(
        result,
        prepared.original_request(),
        IdNamespace([103; 16]),
        SourceFraming::Sse,
        settings().codec,
        settings().events,
    )
    .unwrap() else {
        panic!("rejected")
    };
    let mut reader = NativeReader::new(response.body, SourceFraming::Sse, settings().codec, 1024);
    let mut saw_usage = false;
    while let Some(frame) = ready(reader.next::<h::stream::ChatCompletionChunk>()).unwrap() {
        if let NativeFrame::Event { value, .. } = frame {
            saw_usage |= value.usage.flatten().is_some();
            // The call's alias names the upstream ID itself.
            for choice in value.choices {
                for tool in choice.delta.tool_calls.flatten().into_iter().flatten() {
                    if let Some(id) = tool.id.flatten() {
                        assert_eq!(id, "call_gpe_tool_3asource");
                    }
                }
            }
        }
    }
    assert_eq!(saw_usage, include_usage);
    assert_eq!(store.serial.load(Ordering::SeqCst), writes);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn synthesis_cannot_mint_unpersisted_response_item_ids_or_expose_over_budget_stream() {
    let mut body: r::GenerateContentResponseBody =
        serde_json::from_value(all_pairs::native_response(Dialect::OpenAi)).unwrap();
    for item in &mut body.output {
        if let r::ResponseOutputItem::FunctionCall(call) = item {
            call.id = None;
        }
    }
    let result = GenerationOutcome::Success {
        response: WireResponse {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        },
        report: Report::default(),
    };
    match synthesize::synthesize(
        result,
        IdNamespace([103; 16]),
        SourceFraming::Sse,
        settings().codec,
        settings().events,
    ) {
        Err(error) => assert_eq!(error.kind(), TransformErrorKind::MissingState),
        Ok(_) => panic!("allocated unsaved item IDs"),
    }
    let body: h::GenerateContentResponseBody =
        serde_json::from_value(all_pairs::native_response(Dialect::OpenAiChat)).unwrap();
    let result = GenerationOutcome::Success {
        response: WireResponse {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        },
        report: Report::default(),
    };
    let mut limits = settings().codec;
    limits.max_body_bytes = 10;
    match synthesize::synthesize(
        result,
        IdNamespace([103; 16]),
        SourceFraming::Sse,
        limits,
        settings().events,
    ) {
        Err(error) => assert_eq!(error.kind(), TransformErrorKind::Limit),
        Ok(_) => panic!("exposed partial over-budget synthesis"),
    }
}
