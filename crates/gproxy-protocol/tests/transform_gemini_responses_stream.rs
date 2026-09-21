use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::{
            gemini_responses::{self as pair, stream::*, *},
            stream::{
                gemini::GeminiStreamCollector,
                responses::{ResponsesStreamCollector, synthesize_responses_stream},
            },
        },
        identity::*,
    },
    wire::{
        gemini as g,
        openai::responses::{input as i, response as r, stream as s},
    },
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace([73; 16]))
}
fn context() -> GeminiResponseContext {
    GeminiResponseContext{request:serde_json::from_value(json!({"model":"client-alias","input":"task","instructions":"original rules","parallel_tool_calls":false,"tool_choice":"auto","metadata":{"real":"kept"},"temperature":0.4,"foreign":"extension_marker"})).unwrap(),effective_parallel_tool_calls:false,effective_tool_choice:i::ToolChoice::Mode(i::ToolChoiceMode::Auto),usage:GeminiUsageFacts{cache_write_tokens:Some(2),cached_tokens:None},created_at:123,effective_prompt_cache_options:None}
}
fn gc() -> GeminiToResponsesContext {
    GeminiToResponsesContext {
        response: context(),
        actual_model: None,
        final_thinking_tokens: None,
    }
}
fn ge(v: Value) -> g::GenerateContentResponseBody {
    serde_json::from_value(v).unwrap()
}
fn usage() -> Value {
    json!({"promptTokenCount":9,"cachedContentTokenCount":4,"candidatesTokenCount":4,"thoughtsTokenCount":1,"totalTokenCount":14})
}
fn gfinish(reason: &str) -> g::GenerateContentResponseBody {
    ge(json!({"candidates":[{"index":0,"finishReason":reason}],"usageMetadata":usage()}))
}
fn gpart(parts: Value) -> g::GenerateContentResponseBody {
    ge(
        json!({"modelVersion":"actual-model","candidates":[{"index":0,"content":{"role":"model","parts":parts}}]}),
    )
}
fn response(output: Value, status: &str, reason: Option<&str>) -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"response-source","object":"response","created_at":123,"model":"actual-model","output":output,"status":status,"error":null,"incomplete_details":reason.map(|reason|json!({"reason":reason})),"instructions":null,"metadata":null,"temperature":null,"top_p":null,"parallel_tool_calls":false,"tool_choice":"auto","tools":[],"usage":{"input_tokens":9,"input_tokens_details":{"cache_write_tokens":2,"cached_tokens":4},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":1},"total_tokens":14}})).unwrap()
}
fn message(parts: Value) -> Value {
    json!({"type":"message","id":"msg-source","role":"assistant","status":"completed","content":parts})
}
fn text(v: &str) -> Value {
    json!({"type":"output_text","text":v,"annotations":[],"logprobs":[]})
}
fn function(id: &str, call: &str, args: &str) -> Value {
    json!({"type":"function_call","id":id,"call_id":call,"name":"same","arguments":args,"status":"completed"})
}
fn r_events(body: r::GenerateContentResponseBody) -> Vec<s::StreamEvent> {
    synthesize_responses_stream(body, &mut flow(), Default::default())
        .unwrap()
        .value
}
fn collect_g(chunks: Vec<g::GenerateContentResponseBody>) -> g::GenerateContentResponseBody {
    let mut c = GeminiStreamCollector::new(Default::default());
    for v in chunks {
        c.push(v).unwrap();
    }
    c.finish().unwrap().value
}
fn collect_r(chunks: Vec<s::StreamEvent>) -> r::GenerateContentResponseBody {
    let mut c = ResponsesStreamCollector::new(Default::default());
    for v in chunks {
        c.push(v).unwrap();
    }
    c.finish().unwrap().value
}
fn normalize_g(body: g::GenerateContentResponseBody) -> Value {
    let mut value = serde_json::to_value(body).unwrap();
    for candidate in value["candidates"].as_array_mut().unwrap() {
        if let Some(parts) = candidate["content"]["parts"].as_array_mut() {
            let mut normalized: Vec<Value> = Vec::new();
            for part in std::mem::take(parts) {
                let plain = |v: &Value| {
                    v.get("text").is_some()
                        && v.as_object()
                            .unwrap()
                            .keys()
                            .all(|k| k == "text" || k == "thought")
                };
                if plain(&part)
                    && normalized
                        .last()
                        .is_some_and(|old| plain(old) && old.get("thought") == part.get("thought"))
                {
                    let old = normalized.last_mut().unwrap();
                    old["text"] = json!(format!(
                        "{}{}",
                        old["text"].as_str().unwrap(),
                        part["text"].as_str().unwrap()
                    ));
                } else {
                    normalized.push(part);
                }
            }
            *parts = normalized;
        }
    }
    value
}
fn run_g(chunks: Vec<g::GenerateContentResponseBody>) -> r::GenerateContentResponseBody {
    let source = collect_g(chunks.clone());
    let mut stream = GeminiToResponsesStream::new(gc(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for chunk in chunks {
        out.extend(stream.push(chunk).unwrap().value);
    }
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let actual = collect_r(out);
    let expected = pair::gemini_to_responses_response(
        source,
        context(),
        &mut end.identities.clone(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
    actual
}
fn run_r(events: Vec<s::StreamEvent>) -> g::GenerateContentResponseBody {
    let source = collect_r(events.clone());
    let mut stream =
        ResponsesToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for event in events {
        out.extend(stream.push(event).unwrap().value);
    }
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let actual = collect_g(out);
    let expected = pair::responses_to_gemini_response(source, Default::default())
        .unwrap()
        .value;
    assert_eq!(normalize_g(actual.clone()), normalize_g(expected));
    actual
}
fn renumber(events: Vec<s::StreamEvent>) -> Vec<s::StreamEvent> {
    events
        .into_iter()
        .enumerate()
        .map(|(n, e)| {
            let mut v = serde_json::to_value(e).unwrap();
            v["sequence_number"] = json!(n);
            serde_json::from_value(v).unwrap()
        })
        .collect()
}
#[path = "transform_gemini_responses_stream/gemini.rs"]
mod gemini;
#[path = "transform_gemini_responses_stream/limits.rs"]
mod limits;
#[path = "transform_gemini_responses_stream/responses.rs"]
mod responses;

#[path = "transform_gemini_responses_stream/images.rs"]
mod images;
