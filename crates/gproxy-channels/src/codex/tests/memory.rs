use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, UsageCtx};
use gproxy_protocol::{Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::super::CodexChannel;

const MEMORY: OperationKey = OperationKey::family(Operation::SummarizeMemory, WireFamily::OpenAi);

#[test]
fn descriptor_prepares_typed_memory_summary_for_estimated_settlement() {
    let supports = CodexChannel.descriptor().supports;
    assert!(
        supports
            .iter()
            .any(|support| support.source == MEMORY && support.target == MEMORY)
    );

    let mut headers = HeaderMap::new();
    headers.insert("x-openai-memgen-request", "true".parse().unwrap());
    headers.insert("x-openai-subagent", "memory".parse().unwrap());
    let body = Bytes::from(
        json!({
            "model":"route",
            "traces":[{
                "id":"trace-1",
                "metadata":{"source_path":"/tmp/trace.jsonl", "future_metadata":true},
                "items":[{"type":"future_item"}],
                "future_trace":1
            }],
            "future_request":true
        })
        .to_string(),
    );
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            key: MEMORY,
            stream: false,
            method: &Method::POST,
            path: "/v1/memories/trace_summarize",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "gpt-5.4",
            provider_settings: &json!({}),
            secret: &json!({"access_token":"token", "account_id":"account"}),
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://chatgpt.com/backend-api/codex/memories/trace_summarize"
    );
    assert_eq!(
        prepared.request.headers()["x-openai-memgen-request"],
        "true"
    );
    assert_eq!(prepared.request.headers()["x-openai-subagent"], "memory");
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "gpt-5.4");
    assert_eq!(shaped["traces"][0]["future_trace"], 1);
    assert_eq!(shaped["traces"][0]["metadata"]["future_metadata"], true);
    assert_eq!(shaped["future_request"], true);
    assert!(shaped.get("reasoning").is_none());

    let response = Bytes::from_static(br#"{"output":[]}"#);
    assert!(
        super::super::usage::from_body(UsageCtx {
            key: MEMORY,
            request_body: &body,
            response_headers: &HeaderMap::new(),
            response_body: &response,
        })
        .is_none()
    );
}
