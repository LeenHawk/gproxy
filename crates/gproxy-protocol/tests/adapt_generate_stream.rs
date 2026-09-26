#[path = "support/client_tools.rs"]
mod client_tools_base;
#[path = "adapt_generate_stream/host.rs"]
mod host;
use gproxy_protocol::{
    Dialect,
    adapt::generate::{
        chat_claude::ChatViaClaude,
        stream::{event::EventLimits, reader::SourceFraming, *},
        *,
    },
    codec::CodecLimits,
    transform::{
        TransformErrorKind,
        generate::claude_chat::stream::ClaudeToChatContext,
        identity::{IdNamespace, IdSyntax, IdentityRole, IdentityTarget},
    },
    wire::openai::chat as h,
};
use host::*;
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::{Arc, atomic::Ordering},
    task::{Context, Waker},
    time::{Duration, UNIX_EPOCH},
};
fn state(store: &Store) -> GenerationStateAccess<'_, Store> {
    GenerationStateAccess {
        store,
        scope: &(),
        target: IdentityTarget::new("selected", Dialect::Claude)
            .unwrap()
            .with_origin("origin")
            .unwrap(),
        conversation_key: "conversation".into(),
        now: UNIX_EPOCH,
        expires_at: UNIX_EPOCH + Duration::from_secs(1000),
        max_records: 64,
    }
}
fn selected() -> StreamTarget {
    let mut identities = GenerationIdentity::new(
        IdNamespace([61; 16]),
        IdNamespace([62; 16]),
        Dialect::OpenAiChat,
        Dialect::Claude,
    )
    .unwrap();
    identities.response_policy = identities
        .response_policy
        .with_syntax(IdSyntax::AsciiIdentifier);
    StreamTarget {
        endpoint: Endpoint::new("/messages").unwrap(),
        identities,
    }
}
fn settings() -> StreamSettings {
    StreamSettings {
        codec: CodecLimits {
            max_buffer_bytes: 65536,
            max_value_bytes: 65536,
            max_body_bytes: 262144,
            max_line_bytes: 65536,
            max_part_bytes: 65536,
            max_parts: 64,
        },
        events: EventLimits {
            max_events: 1024,
            max_bytes: 262144,
            max_pending_bytes: 65536,
            max_items: 64,
            max_parts: 64,
            max_tools: 64,
            max_choices: 8,
        },
        source_framing: SourceFraming::Sse,
        client_framing: SourceFraming::Sse,
    }
}
fn request(include_usage: bool) -> h::GenerateContentRequestBody {
    serde_json::from_value(json!({"model":"client-alias","stream":true,"stream_options":{"include_usage":include_usage},"max_completion_tokens":64,"messages":[{"role":"user","content":"hi","nested_sentinel":"DROP"}],"top_sentinel":"DROP"})).unwrap()
}
fn event(feed: &Feed, value: Value) {
    feed.push(format!(
        "event: {}\ndata: {}\n\n",
        value["type"].as_str().unwrap(),
        value
    ));
}
fn prefix(feed: &Feed) {
    event(
        feed,
        json!({"type":"message_start","message":{"type":"message","id":"message:original","model":"selected","role":"assistant","content":[],"usage":{"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}},"nested_sentinel":"DROP"},"top_sentinel":"DROP"}),
    );
    event(
        feed,
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
    );
    event(
        feed,
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}),
    );
    event(feed, json!({"type":"content_block_stop","index":0}));
}
fn terminal(feed: &Feed) {
    event(
        feed,
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":2,"output_tokens_details":{"thinking_tokens":0}}}),
    );
    event(feed, json!({"type":"message_stop"}));
    feed.close();
}
fn prepared(
    store: &Store,
    usage: bool,
) -> StreamInvocation<gproxy_protocol::transform::generate::claude_chat::stream::ClaudeToChatStream>
{
    ready(ChatViaClaude::prepare_stream(
        request(usage),
        selected(),
        ClaudeToChatContext { created: 7 },
        settings(),
        &state(store),
    ))
    .unwrap()
}
#[test]
fn actual_post_streams_before_eof_and_alias_is_durable_before_every_yield() {
    let store = Arc::new(Store::default());
    let feed = Feed::default();
    prefix(&feed);
    let host = Host::stream(store.clone(), feed.clone());
    let access = state(&store);
    let mut call = prepared(&store, true);
    assert!(matches!(
        ready(call.start(&host, &(), &access)).unwrap(),
        StreamStart::Streaming(_)
    ));
    let sent: Value = serde_json::from_slice(&host.sent.lock().unwrap()[0].body).unwrap();
    assert_eq!(sent["stream"], true);
    assert_eq!(sent["model"], "selected");
    assert!(!sent.to_string().contains("sentinel"));
    let mut bytes = Vec::new();
    let mut saw_text = false;
    for _ in 0..5 {
        let chunk = ready(call.next(&access)).unwrap().unwrap();
        if let Some(event) = &chunk.event {
            assert_ne!(event.id, "message:original");
            let saved = ready(access.read(IdentityRole::Response, &event.id))
                .unwrap()
                .unwrap();
            assert_eq!(saved.response_id.as_deref(), Some("message:original"));
        }
        bytes.extend_from_slice(&chunk.bytes);
        if String::from_utf8_lossy(&bytes).contains("hello") {
            saw_text = true;
            break;
        }
    }
    assert!(saw_text);
    assert!(call.client_result().is_none());
    terminal(&feed);
    while let Some(chunk) = ready(call.next(&access)).unwrap() {
        bytes.extend_from_slice(&chunk.bytes);
    }
    assert!(bytes.ends_with(b"data: [DONE]\n\n"));
    assert_eq!(
        call.client_result()
            .unwrap()
            .usage
            .as_ref()
            .unwrap()
            .total_tokens,
        5
    );
    assert!(!String::from_utf8_lossy(&bytes).contains("sentinel"));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert_eq!(
        ready(call.start(&host, &(), &access)).unwrap_err().kind(),
        TransformErrorKind::Conflict
    );
}
#[test]
fn applied_alias_cas_cancellation_retains_exact_event_and_does_not_read_or_post_again() {
    let store = Arc::new(Store::default());
    let feed = Feed::default();
    prefix(&feed);
    let host = Host::stream(store.clone(), feed.clone());
    let access = state(&store);
    let mut call = prepared(&store, true);
    ready(call.start(&host, &(), &access)).unwrap();
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
    let reads = feed.polls();
    let writes = store.serial.load(Ordering::SeqCst);
    let event = ready(call.next(&access)).unwrap().unwrap().event.unwrap();
    assert_eq!(feed.polls(), reads);
    assert_eq!(store.serial.load(Ordering::SeqCst), writes);
    assert!(
        ready(access.read(IdentityRole::Response, &event.id))
            .unwrap()
            .is_some()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn cancelled_reservation_resumes_unsent_call_but_fresh_duplicate_namespace_cannot_post() {
    let store = Arc::new(Store::default());
    let feed = Feed::default();
    let host = Host::stream(store.clone(), feed);
    let access = state(&store);
    let mut call = prepared(&store, true);
    store.hang_applied.store(true, Ordering::SeqCst);
    {
        let mut start = Box::pin(call.start(&host, &(), &access));
        assert!(
            start
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert!(!call.send_started());
    assert!(host.sent.lock().unwrap().is_empty());
    ready(call.start(&host, &(), &access)).unwrap();
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    let duplicate = ready(ChatViaClaude::prepare_stream(
        request(true),
        selected(),
        ClaudeToChatContext { created: 7 },
        settings(),
        &access,
    ));
    assert_eq!(
        duplicate
            .err()
            .expect("duplicate namespace preparation must fail")
            .kind(),
        TransformErrorKind::Conflict
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn client_usage_flag_is_honored_and_incomplete_source_never_emits_terminal_success() {
    for complete in [true, false] {
        let store = Arc::new(Store::default());
        let feed = Feed::default();
        prefix(&feed);
        if complete {
            terminal(&feed);
        } else {
            feed.close();
        }
        let host = Host::stream(store.clone(), feed);
        let access = state(&store);
        let mut call = prepared(&store, false);
        ready(call.start(&host, &(), &access)).unwrap();
        let mut output = Vec::new();
        let mut failed = false;
        loop {
            match ready(call.next(&access)) {
                Ok(Some(chunk)) => output.extend_from_slice(&chunk.bytes),
                Ok(None) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        assert_eq!(failed, !complete);
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.contains("[DONE]"), complete);
        if complete {
            assert!(call.client_result().unwrap().usage.is_none());
        } else {
            assert!(!text.contains("\"finish_reason\":\"stop\""));
        }
    }
}

#[path = "adapt_generate_stream/all_pairs.rs"]
mod all_pairs;

#[path = "adapt_generate_stream/gemini_thinking.rs"]
mod gemini_thinking;

#[path = "adapt_generate_stream/framing.rs"]
mod framing;

#[path = "adapt_generate_stream/parity.rs"]
mod parity;

#[path = "adapt_generate_stream/synthesis.rs"]
mod synthesis;

#[path = "adapt_generate_stream/websocket.rs"]
mod websocket;

#[path = "adapt_generate_stream/fanout.rs"]
mod fanout;

#[path = "adapt_generate_stream/instructions.rs"]
mod instructions;

#[path = "adapt_generate_stream/tool_reference.rs"]
mod tool_reference;

#[path = "adapt_generate_stream/custom_tools.rs"]
mod custom_tools;

#[path = "adapt_generate_stream/client_tools.rs"]
mod client_tools;

#[path = "adapt_generate_stream/client_tools_backends.rs"]
mod client_tools_backends;
