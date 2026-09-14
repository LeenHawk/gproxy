#[path = "adapt_generate_image_resources/host.rs"]
mod host;
#[allow(dead_code)]
#[path = "adapt_generate_stream/host.rs"]
mod http_host;
#[path = "adapt_generate_image_resources/read_cancel.rs"]
mod read_cancel;
use gproxy_protocol::{
    Dialect, HttpBody, WireResponse,
    adapt::generate::{
        gemini_responses::*,
        image_resources::*,
        stream::{event::EventLimits, reader::SourceFraming, *},
        *,
    },
    codec::CodecLimits,
    transform::{
        generate::gemini_responses as pair,
        identity::{IdNamespace, IdentityTarget},
    },
    wire::{gemini as g, openai::responses as r},
};
use host::*;
use http_host::{Feed, Host, Store, ready};
use serde_json::json;
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
    task::{Context, Waker},
    time::{Duration, UNIX_EPOCH},
};
fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 65536,
        max_value_bytes: 65536,
        max_body_bytes: 262144,
        max_line_bytes: 65536,
        max_part_bytes: 65536,
        max_parts: 64,
    }
}
fn settings() -> StreamSettings {
    StreamSettings {
        codec: limits(),
        source_framing: SourceFraming::Sse,
        client_framing: SourceFraming::Sse,
        events: EventLimits {
            max_events: 1024,
            max_bytes: 262144,
            max_pending_bytes: 65536,
            max_items: 64,
            max_tools: 64,
            max_parts: 64,
            max_choices: 1,
        },
    }
}
fn state(store: &Store, dialect: Dialect) -> GenerationStateAccess<'_, Store> {
    GenerationStateAccess {
        store,
        scope: &(),
        target: IdentityTarget::new("selected", dialect)
            .unwrap()
            .with_origin("origin")
            .unwrap(),
        conversation_key: "image-conversation".into(),
        now: UNIX_EPOCH,
        expires_at: UNIX_EPOCH + Duration::from_secs(1000),
        max_records: 64,
    }
}
fn resources(host: &Resources) -> GenerationResources<'_, Resources> {
    GenerationResources {
        access: host,
        scope: &1,
        limits: limits(),
        max_references: 8,
        now: UNIX_EPOCH,
    }
}
fn ids(client: Dialect, native: Dialect) -> GenerationIdentity {
    GenerationIdentity::new(
        IdNamespace::with_bytes([121; 16]),
        IdNamespace::with_bytes([122; 16]),
        client,
        native,
    )
    .unwrap()
}
fn rrequest() -> r::GenerateContentRequestBody {
    serde_json::from_value(json!({"model":"client","input":"draw","tools":[{"type":"image_generation"}],"tool_choice":"auto"})).unwrap()
}
fn grequest() -> g::GenerateContentRequestBody {
    serde_json::from_value(json!({"contents":[{"role":"user","parts":[{"text":"draw"}]}],
        "generationConfig":{"responseModalities":["TEXT","IMAGE"],"responseFormat":{"image":{"delivery":"URI"}}}})).unwrap()
}
fn gresponse() -> g::GenerateContentResponseBody {
    serde_json::from_value(json!({"responseId":"g-image","modelVersion":"selected","candidates":[{"index":0,
        "content":{"role":"model","parts":[{"fileData":{"fileUri":"gs://private/generated.png","mimeType":"image/png"},"thoughtSignature":"original-signed-file"}]},"finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5,"cachedContentTokenCount":0,"thoughtsTokenCount":0,"toolUsePromptTokenCount":0}})).unwrap()
}
fn rresponse() -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"r-image","object":"response","created_at":7,"model":"selected","status":"completed","error":null,"incomplete_details":null,
        "instructions":null,"metadata":null,"parallel_tool_calls":false,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[{"type":"image_generation"}],
        "output":[{"type":"image_generation_call","id":"ig_source","result":PNG,"status":"completed"}],
        "usage":{"input_tokens":3,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":2,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":5}})).unwrap()
}
fn facts() -> GeminiReturnFacts {
    GeminiReturnFacts {
        parallel_tool_calls: false,
        tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
        prompt_cache_options: None,
        usage: pair::GeminiUsageFacts {
            cache_write_tokens: Some(0),
            cached_tokens: None,
        },
        created_at: 7,
    }
}
fn host<N: serde::Serialize>(store: Arc<Store>, native: &N) -> Host {
    let mut host = Host::stream(store, Feed::default());
    host.require_reservation = false;
    *host.response.lock().unwrap() = Some(WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Bytes(serde_json::to_vec(native).unwrap().into()),
    });
    host
}
#[test]
fn signed_file_output_uses_actual_read_and_replays_the_original_file_part() {
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::Gemini);
    let resource_host = Resources::default();
    let resources = resources(&resource_host);
    let native = gresponse();
    let host = host(store.clone(), &native);
    let mut call = ResponsesViaGemini::prepare(
        rrequest(),
        "selected",
        Endpoint::new("/generateContent").unwrap(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let mut progress = ImageResourceProgress::default();
    let GenerationOutcome::Success { response, .. } = ready(call.invoke_with_image_resources(
        &host,
        &(),
        limits(),
        &state,
        &resources,
        &mut progress,
        |_| Ok(facts()),
    ))
    .unwrap() else {
        panic!()
    };
    let r::ResponseOutputItem::ImageGenerationCall(image) = &response.body.output[0] else {
        panic!()
    };
    assert_eq!(image.result.as_deref(), Some(PNG));
    assert_eq!(resource_host.reads.load(Ordering::SeqCst), 1);
    let mut input = rrequest();
    input.input = Some(r::Input::Items(vec![r::InputItem::ImageGenerationCall(
        image.clone(),
    )]));
    let prepared = ready(ResponsesViaGemini::prepare_with_state(
        input.clone(),
        "selected",
        Endpoint::new("/generateContent").unwrap(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        &state,
        Default::default(),
    ))
    .unwrap();
    let restored = &prepared.target_request().contents[0]
        .parts
        .as_ref()
        .unwrap()[0];
    assert_eq!(
        restored,
        &native.candidates.as_ref().unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()[0]
    );
    assert!(restored.inline_data.is_none());
    ready(
        call.recover_with_image_resources(limits(), &state, &resources, &mut progress, |_| {
            Ok(facts())
        }),
    )
    .unwrap();
    assert_eq!(resource_host.reads.load(Ordering::SeqCst), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn uri_publication_cancellation_queries_receipt_and_never_repeats_post_or_publish() {
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::OpenAi);
    let resource_host = Resources::default();
    resource_host.hang_publish.store(true, Ordering::SeqCst);
    let resources = resources(&resource_host);
    let native = rresponse();
    let host = host(store.clone(), &native);
    let mut call = ready(GeminiViaResponses::prepare_with_image_resources(
        grequest(),
        "selected",
        Endpoint::new("/responses").unwrap(),
        ids(Dialect::Gemini, Dialect::OpenAi),
        &state,
        &resources,
    ))
    .unwrap();
    assert!(call.convert_response(native, Default::default()).is_err());
    let mut progress = ImageResourceProgress::default();
    {
        let mut pending = Box::pin(call.invoke_with_image_resources(
            &host,
            &(),
            limits(),
            &state,
            &resources,
            &mut progress,
            |_| Ok(Default::default()),
        ));
        assert!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(resource_host.hung.load(Ordering::SeqCst));
    }
    assert_eq!(progress.publications.operation_ids().count(), 1);
    let GenerationOutcome::Success { response, .. } = ready(call.recover_with_image_resources(
        limits(),
        &state,
        &resources,
        &mut progress,
        |_| Ok(Default::default()),
    ))
    .unwrap() else {
        panic!()
    };
    let image = &response.body.candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0];
    assert!(image.inline_data.is_none());
    assert!(
        image
            .file_data
            .as_ref()
            .unwrap()
            .file_uri
            .starts_with("https://published.example/")
    );
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    assert_eq!(resource_host.statuses.load(Ordering::SeqCst), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    let wrong = GenerationResources {
        scope: &2,
        ..resources
    };
    assert!(
        ready(
            call.recover_with_image_resources(limits(), &state, &wrong, &mut progress, |_| Ok(
                Default::default()
            ))
        )
        .is_err()
    );
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    resource_host.contradict_publication_metadata();
    let statuses = resource_host.statuses.load(Ordering::SeqCst);
    let receipts = progress.publications.receipts().len();
    for _ in 0..3 {
        assert!(
            ready(call.recover_with_image_resources(
                limits(),
                &state,
                &resources,
                &mut progress,
                |_| Ok(Default::default())
            ))
            .is_err()
        );
    }
    assert_eq!(resource_host.statuses.load(Ordering::SeqCst), statuses + 1);
    assert_eq!(progress.publications.receipts().len(), receipts + 1);
}
#[test]
fn file_image_stream_exposes_validated_bytes_before_eof_and_saves_original_proof() {
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::Gemini);
    let resource_host = Resources::default();
    let resources = resources(&resource_host);
    let request = rrequest();
    let f = facts();
    let mut call = ready(ResponsesViaGemini::prepare_stream(
        request.clone(),
        StreamTarget {
            model: "selected".into(),
            endpoint: Endpoint::new("/streamGenerateContent").unwrap(),
            identities: ids(Dialect::OpenAi, Dialect::Gemini),
        },
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: pair::stream::GeminiToResponsesContext {
                response: pair::GeminiResponseContext {
                    request,
                    effective_parallel_tool_calls: f.parallel_tool_calls,
                    effective_tool_choice: f.tool_choice,
                    effective_prompt_cache_options: f.prompt_cache_options,
                    usage: f.usage,
                    created_at: f.created_at,
                },
                actual_model: Some("selected".into()),
                final_thinking_tokens: None,
            },
        },
        settings(),
        &state,
    ))
    .unwrap();
    let feed = Feed::default();
    let mut prefix = gresponse();
    prefix.candidates.as_mut().unwrap()[0].finish_reason = None;
    prefix.usage_metadata = None;
    feed.push(format!(
        "data: {}\n\n",
        serde_json::to_string(&prefix).unwrap()
    ));
    let host = Host::stream(store.clone(), feed.clone());
    ready(call.start(&host, &(), &state)).unwrap();
    let mut progress = ImageStreamProgress::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        fn assert_send<T: Send>(_: &T) {}
        let normal = call.next(&state);
        assert_send(&normal);
        drop(normal);
        let mapped = call.next_with_image_resources(&state, &resources, &mut progress);
        assert_send(&mapped);
        drop(mapped);
    }
    loop {
        let chunk = ready(call.next_with_image_resources(&state, &resources, &mut progress))
            .unwrap()
            .unwrap();
        if String::from_utf8_lossy(&chunk.bytes).contains(PNG) {
            break;
        }
    }
    assert!(call.client_result().is_none());
    assert_eq!(resource_host.reads.load(Ordering::SeqCst), 1);
    let mut terminal = gresponse();
    terminal.candidates.as_mut().unwrap()[0].content = None;
    feed.push(format!(
        "data: {}\n\n",
        serde_json::to_string(&terminal).unwrap()
    ));
    feed.close();
    while let Some(chunk) =
        ready(call.next_with_image_resources(&state, &resources, &mut progress)).unwrap()
    {
        if matches!(chunk.event, Some(r::stream::StreamEvent::Completed(_))) {
            assert!(
                store
                    .entries
                    .lock()
                    .unwrap()
                    .keys()
                    .any(|key| key.ends_with(":gemini-image-file"))
            );
        }
    }
    let original = &call.native_result().unwrap().candidates.as_ref().unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0];
    assert!(original.file_data.is_some());
    assert!(original.inline_data.is_none());
    assert_eq!(
        original.thought_signature.as_deref(),
        Some("original-signed-file")
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn uri_stream_retains_pending_client_event_across_uncertain_publication() {
    let store = Arc::new(Store::default());
    let state = state(&store, Dialect::OpenAi);
    let resource_host = Resources::default();
    resource_host.hang_publish.store(true, Ordering::SeqCst);
    let resources = resources(&resource_host);
    let mut call = ready(GeminiViaResponses::prepare_stream_with_capabilities(
        grequest(),
        StreamTarget {
            model: "selected".into(),
            endpoint: Endpoint::new("/responses").unwrap(),
            identities: ids(Dialect::Gemini, Dialect::OpenAi),
        },
        Default::default(),
        settings(),
        &state,
        &resources,
    ))
    .unwrap();
    let feed = Feed::default();
    let events =
        gproxy_protocol::transform::generate::stream::responses::synthesize_responses_stream(
            rresponse(),
            &mut gproxy_protocol::transform::identity::IdentityFlow::new(IdNamespace::with_bytes(
                [125; 16],
            )),
            Default::default(),
        )
        .unwrap()
        .value;
    for event in events {
        feed.push(format!(
            "data: {}\n\n",
            serde_json::to_string(&event).unwrap()
        ));
    }
    let host = Host::stream(store.clone(), feed.clone());
    ready(call.start(&host, &(), &state)).unwrap();
    assert!(
        ready(call.next(&state)).is_err(),
        "ordinary next must not bypass URI publication"
    );
    let mut progress = ImageStreamProgress::default();
    for _ in 0..10000 {
        let mut pending =
            Box::pin(call.next_with_image_resources(&state, &resources, &mut progress));
        match pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            std::task::Poll::Ready(Ok(Some(chunk))) => {
                assert!(!String::from_utf8_lossy(&chunk.bytes).contains(PNG))
            }
            std::task::Poll::Pending => {}
            _ => panic!("unexpected stream outcome before image publication"),
        }
        if resource_host.hung.load(Ordering::SeqCst) {
            break;
        }
    }
    assert!(resource_host.hung.load(Ordering::SeqCst));
    let mut fresh = ImageStreamProgress::default();
    assert!(ready(call.next_with_image_resources(&state, &resources, &mut fresh)).is_err());
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    let chunk = ready(call.next_with_image_resources(&state, &resources, &mut progress))
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&chunk.bytes).contains("https://published.example/"));
    feed.close();
    while ready(call.next_with_image_resources(&state, &resources, &mut progress))
        .unwrap()
        .is_some()
    {}
    assert!(call.client_result().is_some());
    assert_eq!(resource_host.publishes.load(Ordering::SeqCst), 1);
    assert_eq!(resource_host.statuses.load(Ordering::SeqCst), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
