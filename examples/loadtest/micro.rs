//! `--micro`: single-event conversion micro-benchmarks, isolating the cost of
//! the Value round trip in the streaming dispatch path.
//!
//! Path A (current pipeline, `dispatch::run_value`):
//!   `from_str::<Value>` → `from_value::<S>` → convert → `to_value` → `to_string`
//! Path B (proposed direct path):
//!   `from_str::<S>` → convert → `to_string`
//!
//! A − B is the exact per-event saving of dropping the Value legs. The bare
//! parse legs are reported separately for context.

use std::hint::black_box;
use std::time::{Duration, Instant};

use gproxy::protocol::{ContentGenerationKind as CGK, Operation, OperationKey};
use gproxy::transform::generate_content as gc;
use gproxy::transform::{TransformContext, TransformError};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Representative content-delta frames (the by-far hottest event shape).
const CHAT_DELTA: &str = r#"{"id":"chatcmpl-1","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"hello world token span"},"finish_reason":null}]}"#;
const CLAUDE_DELTA: &str = r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello world token span"}}"#;
const GEMINI_DELTA: &str = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"hello world token span"}]},"index":0}],"modelVersion":"gemini-test"}"#;

pub fn run() {
    println!("micro: per-event conversion cost (single thread, ns/op)");
    println!(
        "{:<24} {:>10} {:>10} {:>10} {:>7}",
        "pair (upstream->inbound)", "A value", "B direct", "saving", "save%"
    );
    run_pair(
        "chat->claude",
        CHAT_DELTA,
        &ctx(CGK::OpenAiChatCompletions, CGK::ClaudeMessages),
        gc::openai_chat_to_claude_messages::stream_event,
    );
    run_pair(
        "chat->responses",
        CHAT_DELTA,
        &ctx(CGK::OpenAiChatCompletions, CGK::OpenAiResponses),
        gc::openai_chat_to_openai_responses::stream_event,
    );
    run_pair(
        "claude->chat",
        CLAUDE_DELTA,
        &ctx(CGK::ClaudeMessages, CGK::OpenAiChatCompletions),
        gc::claude_messages_to_openai_chat::stream_event,
    );
    run_pair(
        "gemini->chat",
        GEMINI_DELTA,
        &ctx(CGK::GeminiGenerateContent, CGK::OpenAiChatCompletions),
        gc::gemini_generate_content_to_openai_chat::stream_event,
    );
    println!();
    println!("parse legs (context):");
    leg("chat delta  -> Value", || {
        black_box(serde_json::from_str::<Value>(black_box(CHAT_DELTA)).unwrap())
    });
    leg("chat delta  -> typed", || {
        black_box(
            serde_json::from_str::<gproxy::protocol::openai::ChatCompletionChunk>(black_box(
                CHAT_DELTA,
            ))
            .unwrap(),
        )
    });
    leg("claude delta-> Value", || {
        black_box(serde_json::from_str::<Value>(black_box(CLAUDE_DELTA)).unwrap())
    });
}

fn ctx(source: CGK, target: CGK) -> TransformContext {
    // Conversion direction is upstream wire -> inbound wire (response side).
    TransformContext::new(
        OperationKey::content_generation(Operation::StreamGenerateContent, source),
        OperationKey::content_generation(Operation::StreamGenerateContent, target),
    )
}

fn run_pair<S, T>(
    label: &str,
    raw: &str,
    ctx: &TransformContext,
    convert: impl Fn(S, &TransformContext) -> Result<T, TransformError>,
) where
    S: DeserializeOwned,
    T: Serialize,
{
    // Path A: today's dispatch::run_value data path.
    let a = bench(|| {
        let v: Value = serde_json::from_str(black_box(raw)).unwrap();
        let s: S = serde_json::from_value(v).unwrap();
        let t = convert(s, ctx).ok().unwrap();
        let out = serde_json::to_value(&t).unwrap();
        black_box(out.to_string())
    });
    // Path B: direct typed path (no Value legs).
    let b = bench(|| {
        let s: S = serde_json::from_str(black_box(raw)).unwrap();
        let t = convert(s, ctx).ok().unwrap();
        black_box(serde_json::to_string(&t).unwrap())
    });
    println!(
        "{:<24} {:>10.0} {:>10.0} {:>10.0} {:>6.1}%",
        label,
        a,
        b,
        a - b,
        (a - b) / a * 100.0
    );
}

fn leg<R>(label: &str, mut f: impl FnMut() -> R) {
    let ns = bench(|| f());
    println!("  {label:<22} {ns:>8.0} ns/op");
}

/// ~300ms measured window after a short warmup; returns ns/op.
fn bench<R>(mut f: impl FnMut() -> R) -> f64 {
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_millis(50) {
        black_box(f());
    }
    let start = Instant::now();
    let mut iters: u64 = 0;
    while start.elapsed() < Duration::from_millis(300) {
        // batch to amortize the clock read
        for _ in 0..64 {
            black_box(f());
        }
        iters += 64;
    }
    start.elapsed().as_nanos() as f64 / iters as f64
}
