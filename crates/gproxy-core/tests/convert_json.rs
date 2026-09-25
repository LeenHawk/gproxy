#![cfg(not(target_arch = "wasm32"))]

//! Buffered JSON conversion families over the attempt loop: embeddings,
//! guardian, compaction and memory summarisation. Each test drives a client
//! dialect into a provider whose only native dialect differs, then checks the
//! native call that went out and the client-dialect body that came back.

mod support;
use support::*;

use gproxy_core::{
    CoreResult, HttpExecution, PlaintextCodec, RequestContext, SecretCodec, UsageState,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use gproxy_store::entity::{
    config::setting,
    upstream::{credential, provider},
};
use http::{HeaderMap, Method, StatusCode};
use sea_orm::Set;
use serde_json::{Value, json};
use std::sync::Arc;

/// A provider named `id` whose channel declares only `dialect`, with one
/// API-key credential `<id>-key`.
async fn seed_provider(h: &Harness, id: &str, dialect: &str) {
    let store = h.core.store();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set(id.into()),
            name: Set(id.into()),
            channel: Set("test".into()),
            base_url: Set(Some(format!("https://{id}.example"))),
            config: Set(json!({"dialects": [dialect]})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .credentials()
        .create_many(vec![credential::ActiveModel {
            id: Set(format!("{id}-key")),
            provider_id: Set(id.into()),
            user_id: Set(Some("u".into())),
            auth_kind: Set("api_key".into()),
            secret: Set(PlaintextCodec
                .seal(id, &json!({"api_key": format!("k-{id}")}))
                .unwrap()),
            metadata: Set(json!({})),
            ..Default::default()
        }])
        .await
        .unwrap();
    // Snapshots publish monotonically; a new revision makes the reload land.
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set((h.core.snapshot().revision.0 + 1) as i64),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

fn wire(path: &str, body: Value) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(serde_json::to_vec(&body).unwrap().into()),
    }
}

fn key(operation: Operation, dialect: Dialect) -> OperationKey {
    OperationKey { operation, dialect }
}

/// The JSON body the scripted client recorded for the `index`th native call.
fn sent_body(h: &Harness, index: usize) -> Value {
    let line = h.client.seen.lines()[index].clone();
    let (_, body) = line.split_once(" body=").unwrap();
    serde_json::from_str(body).unwrap()
}

/// The named facade for the context's operation. `Core::send` is avoided on
/// purpose: its generic match instantiates every operation's execution
/// future in one debug-build stack frame, which overflows a 2 MiB test
/// thread now that the conversion families carry real state.
async fn send(
    h: &Harness,
    ctx: Arc<RequestContext>,
    request: WireRequest<HttpBody>,
) -> CoreResult<HttpExecution> {
    match ctx.operation.operation {
        Operation::CreateEmbedding => h.core.create_embedding(ctx, request).await,
        Operation::BatchCreateEmbedding => h.core.batch_create_embedding(ctx, request).await,
        Operation::GuardianReview => h.core.guardian_review(ctx, request).await,
        Operation::GuardianClassify => h.core.guardian_classify(ctx, request).await,
        Operation::CompactContent => h.core.compact_content(ctx, request).await,
        Operation::SummarizeMemory => h.core.summarize_memory(ctx, request).await,
        Operation::WebSearch => h.core.web_search(ctx, request).await,
        other => panic!("no facade wired for {other:?}"),
    }
}

#[tokio::test]
async fn standalone_search_uses_a_search_only_generation_and_preserves_rejection() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "search", "gemini").await;
    h.script(vec![json_reply(StatusCode::OK, json!({
        "responseId":"search-response", "modelVersion":"m",
        "candidates":[{"content":{"role":"model","parts":[{"text":"Python pathlib: https://docs.python.org/3/library/pathlib.html"}]},"finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":7,"totalTokenCount":12}
    }))]);
    let query = || {
        wire(
            "/v1/search",
            json!({
                "id":"session", "model":"m", "commands":{"search_query":[{"q":"official Python pathlib docs"}]}
            }),
        )
    };
    let (status, body, state) = run(
        &h,
        "search",
        key(Operation::WebSearch, Dialect::OpenAi),
        query(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state, UsageState::Completed);
    assert!(
        body["output"]
            .as_str()
            .unwrap()
            .contains("https://docs.python.org")
    );
    assert!(body.get("encrypted_output").is_none());
    assert_eq!(
        sent_body(&h, 0)["tools"],
        json!([{"googleSearch":{}},{"urlContext":{}}])
    );
    assert_eq!(h.client.seen.lines().len(), 1);

    // An open/find followup need not contain a fresh search query, and a valid
    // upstream result without text remains an empty successful search result.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "responseId":"page-response", "modelVersion":"m",
            "candidates":[{"content":{"role":"model","parts":[]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":0,"totalTokenCount":5}
        }),
    )]);
    let (status, body, state) = run(
        &h, "search", key(Operation::WebSearch, Dialect::OpenAi),
        wire("/v1/search", json!({
            "id":"session", "model":"m",
            "input":"page1 = https://docs.python.org/3/library/pathlib.html",
            "commands":{"open":[{"ref_id":"page1"}], "find":[{"ref_id":"page1","pattern":"read_text"}]}
        })),
    ).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state, UsageState::Completed);
    assert_eq!(body["output"], "");
    let sent = sent_body(&h, 1).to_string();
    assert!(sent.contains("read_text"));
    assert!(sent.contains("https://docs.python.org/3/library/pathlib.html"));

    h.script(vec![json_reply(
        StatusCode::BAD_REQUEST,
        json!({"error":{"code":400,"message":"rejected","status":"INVALID_ARGUMENT"}}),
    )]);
    let (status, body, _) = run(
        &h,
        "search",
        key(Operation::WebSearch, Dialect::OpenAi),
        query(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.get("output").is_none());
}

async fn run(
    h: &Harness,
    provider: &str,
    key: OperationKey,
    request: WireRequest<HttpBody>,
) -> (StatusCode, Value, UsageState) {
    let ctx = h.context_for(provider, key, "r1", 1, None);
    let execution = send(h, ctx, request).await.unwrap();
    let (response, completion) = execution.into_parts();
    let status = response.status;
    let text = read(response.body).await;
    let body = serde_json::from_str(&text).unwrap_or(Value::String(text));
    let report = completion.await.unwrap();
    (status, body, report.state)
}

#[tokio::test]
async fn openai_single_embedding_is_posted_as_gemini_embed_content() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"embedding":{"values":[0.5,0.25]},"usageMetadata":{"promptTokenCount":3}}),
    )]);
    let (status, body, state) = run(
        &h,
        "gem",
        key(Operation::CreateEmbedding, Dialect::OpenAi),
        wire("/v1/embeddings", json!({"model":"alias","input":"hello"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state, UsageState::Completed);
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["index"], 0);
    assert_eq!(body["data"][0]["embedding"], json!([0.5, 0.25]));
    assert_eq!(body["usage"]["prompt_tokens"], 3);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with(
            "POST https://gem.example/v1beta/models/gpt-x:embedContent auth=Bearer k-gem"
        ),
        "{}",
        seen[0]
    );
    assert_eq!(sent_body(&h, 0)["content"]["parts"][0]["text"], "hello");
}

#[tokio::test]
async fn openai_embedding_list_is_batched_into_gemini_batch_embed_contents() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "embeddings":[{"values":[1.0]},{"values":[2.0]},{"values":[3.0]}],
            "usageMetadata":{"promptTokenCount":3}
        }),
    )]);
    let (status, body, _) = run(
        &h,
        "gem",
        key(Operation::CreateEmbedding, Dialect::OpenAi),
        wire(
            "/v1/embeddings",
            json!({"model":"alias","input":["a","b","c"]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    for (index, item) in data.iter().enumerate() {
        assert_eq!(item["index"], index);
        assert_eq!(item["embedding"][0].as_f64(), Some((index + 1) as f64));
    }
    assert_eq!(body["usage"]["total_tokens"], 3);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://gem.example/v1beta/models/gpt-x:batchEmbedContents"),
        "{}",
        seen[0]
    );
    let sent = sent_body(&h, 0);
    let requests = sent["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0]["model"], "models/gpt-x");
    assert_eq!(requests[2]["content"]["parts"][0]["text"], "c");
}

#[tokio::test]
async fn gemini_single_and_batch_embeddings_are_posted_as_openai_embeddings() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "oai", "openai").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "object":"list","model":"actual",
            "data":[{"object":"embedding","index":0,"embedding":[0.75]}],
            "usage":{"prompt_tokens":1,"total_tokens":1}
        }),
    )]);
    let (status, body, _) = run(
        &h,
        "oai",
        key(Operation::CreateEmbedding, Dialect::Gemini),
        wire(
            "/v1beta/models/alias:embedContent",
            json!({"content":{"parts":[{"text":"hi"}]}}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["embedding"]["values"], json!([0.75]));
    let seen = h.client.seen.lines();
    assert!(
        seen[0].starts_with("POST https://oai.example/v1/embeddings auth=Bearer k-oai"),
        "{}",
        seen[0]
    );
    let sent = sent_body(&h, 0);
    assert_eq!(sent["model"], "gpt-x");
    assert_eq!(sent["input"], json!("hi"));

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "object":"list","model":"actual",
            "data":[
                {"object":"embedding","index":0,"embedding":[1.0]},
                {"object":"embedding","index":1,"embedding":[2.0]}
            ],
            "usage":{"prompt_tokens":2,"total_tokens":2}
        }),
    )]);
    let (status, body, _) = run(
        &h,
        "oai",
        key(Operation::BatchCreateEmbedding, Dialect::Gemini),
        wire(
            "/v1beta/models/alias:batchEmbedContents",
            json!({"requests":[
                {"model":"models/alias","content":{"parts":[{"text":"a"}]}},
                {"model":"models/alias","content":{"parts":[{"text":"b"}]}}
            ]}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let embeddings = body["embeddings"].as_array().unwrap();
    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[1]["values"], json!([2.0]));
    let sent = sent_body(&h, 1);
    assert_eq!(sent["input"], json!(["a", "b"]));
}

#[tokio::test]
async fn rejected_embedding_call_returns_the_native_rejection_and_claude_is_unsupported() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    h.script(vec![(
        StatusCode::SERVICE_UNAVAILABLE,
        vec![("content-type", "application/json")],
        vec![b"{\"error\":\"down\"}".as_slice().into()],
    )]);
    let (status, body, _) = run(
        &h,
        "gem",
        key(Operation::CreateEmbedding, Dialect::OpenAi),
        wire("/v1/embeddings", json!({"model":"alias","input":["a","b"]})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "down");
    assert_eq!(h.client.seen.lines().len(), 1);

    let ctx = h.context_for(
        "claude",
        key(Operation::CreateEmbedding, Dialect::OpenAi),
        "r2",
        1,
        None,
    );
    let error = send(
        &h,
        ctx,
        wire("/v1/embeddings", json!({"model":"alias","input":"x"})),
    )
    .await
    .expect_err("Claude has no embeddings API");
    assert!(
        format!("{error:?}").contains("CreateEmbedding"),
        "{error:?}"
    );
}

fn guardian_request(stream: bool) -> Value {
    json!({
        "model":"guardian-client", "instructions":"Assess the action.",
        "input":[
            {"type":"message","id":"msg-1","role":"user","content":[{"type":"input_text","text":"hello"}]},
            {"type":"function_call","id":"item-1","name":"exec_command","arguments":"{\"command\":\"echo hi\"}","call_id":"call-1"}
        ],
        "tools":[{"type":"function","name":"exec_command"}], "tool_choice":"none",
        "parallel_tool_calls":false, "reasoning":null, "store":false, "stream":stream, "include":[]
    })
}

#[tokio::test]
async fn guardian_review_is_generated_on_claude_and_answered_as_a_completed_response() {
    let h = harness(full(), "round_robin").await;
    let verdict = json!({"risk_level":"high","user_authorization":"low","outcome":"deny","rationale":"The action is high risk."});
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "type":"message","id":"m","role":"assistant","model":"gpt-x",
            "content":[{"type":"text","text":verdict.to_string()}],
            "stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":9,"output_tokens":4}
        }),
    )]);
    let (status, body, state) = run(
        &h,
        "claude",
        key(Operation::GuardianReview, Dialect::OpenAi),
        wire("/v1/guardian", guardian_request(false)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(state, UsageState::Completed);
    assert_eq!(body["object"], "response");
    assert_eq!(body["status"], "completed");
    assert_eq!(body["model"], "guardian-client");
    assert_eq!(body["output"][0]["type"], "message");
    assert_eq!(body["output"][0]["status"], "completed");
    let text = body["output"][0]["content"][0]["text"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(text).unwrap();
    assert_eq!(parsed["outcome"], "deny");
    assert_eq!(parsed["risk_level"], "high");
    assert_eq!(body["output_text"], text);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/messages auth=Bearer k1"),
        "{}",
        seen[0]
    );
    let sent = sent_body(&h, 0);
    assert_eq!(sent["model"], "gpt-x");
    assert!(sent.to_string().contains("exec_command"));
}

#[tokio::test]
async fn guardian_classify_is_generated_on_gemini_and_streams_are_refused() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"low"}]},"finishReason":"STOP"}]}),
    )]);
    let (status, body, _) = run(
        &h,
        "gem",
        key(Operation::GuardianClassify, Dialect::OpenAi),
        wire("/v1/guardian-classifier", guardian_request(false)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["output"][0]["content"][0]["text"], "low");
    assert!(
        h.client.seen.lines()[0]
            .starts_with("POST https://gem.example/v1beta/models/gpt-x:generateContent"),
        "{}",
        h.client.seen.lines()[0]
    );

    let ctx = h.context_for(
        "gem",
        key(Operation::GuardianClassify, Dialect::OpenAi),
        "r2",
        1,
        None,
    );
    let error = send(
        &h,
        ctx,
        wire("/v1/guardian-classifier", guardian_request(true)),
    )
    .await
    .expect_err("streamed guardian is not converted");
    assert!(
        format!("{error:?}").contains("guardian.stream"),
        "{error:?}"
    );
    assert_eq!(
        h.client.seen.lines().len(),
        1,
        "no native call for a refused stream"
    );
}

#[tokio::test]
async fn guardian_rejection_is_returned_verbatim() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![(
        StatusCode::TOO_MANY_REQUESTS,
        vec![("content-type", "application/json")],
        vec![b"{\"error\":\"slow down\"}".as_slice().into()],
    )]);
    let (status, body, _) = run(
        &h,
        "claude",
        key(Operation::GuardianReview, Dialect::OpenAi),
        wire("/v1/guardian", guardian_request(false)),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "slow down");
    assert_eq!(h.client.seen.lines().len(), 1);
}

#[tokio::test]
async fn compaction_summarises_the_whole_history_through_claude() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "type":"message","id":"c","role":"assistant","model":"gpt-x",
            "content":[{"type":"text","text":"preserved decisions and unresolved work"}],
            "stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":2,"output_tokens":5}
        }),
    )]);
    let (status, body, state) = run(
        &h,
        "claude",
        key(Operation::CompactContent, Dialect::OpenAi),
        wire(
            "/v1/responses/compact",
            json!({
                "model":"alias","instructions":"original rules","parallel_tool_calls":true,
                "input":[
                    {"type":"message","role":"user","content":[{"type":"input_text","text":"first"}]},
                    {"type":"message","role":"assistant","content":[{"type":"output_text","text":"second"}]}
                ]
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(state, UsageState::Completed);
    assert_eq!(body.as_object().unwrap().len(), 1, "{body}");
    let output = body["output"].as_array().unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["role"], "assistant");
    assert_eq!(
        output[0]["content"][0]["text"],
        "preserved decisions and unresolved work"
    );
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/messages auth=Bearer k1"),
        "{}",
        seen[0]
    );
    let sent = sent_body(&h, 0);
    assert_eq!(sent["model"], "gpt-x");
    let payload = sent.to_string();
    assert!(
        payload.contains("first") && payload.contains("second"),
        "{payload}"
    );
}

fn memory_request() -> Value {
    json!({
        "model":"alias",
        "traces":[
            {"id":"a","metadata":{"source_path":"/tmp/a"},"items":[{"opaque":true},"a"]},
            {"id":"b","metadata":{"source_path":"/tmp/b"},"items":[{"opaque":true},"b"]}
        ]
    })
}

fn gemini_text(text: String) -> Value {
    json!({"candidates":[{"content":{"role":"model","parts":[{"text":text}]},"finishReason":"STOP"}]})
}

#[tokio::test]
async fn memory_traces_are_summarised_one_call_each_in_order_on_gemini() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    let summary = |id: &str| {
        json!({"trace_summary": format!("trace-{id}"), "memory_summary": format!("memory-{id}")})
            .to_string()
    };
    h.script(vec![
        json_reply(StatusCode::OK, gemini_text(summary("a"))),
        json_reply(StatusCode::OK, gemini_text(summary("b"))),
    ]);
    let (status, body, state) = run(
        &h,
        "gem",
        key(Operation::SummarizeMemory, Dialect::OpenAi),
        wire("/v1/memories/trace_summarize", memory_request()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(state, UsageState::Completed);
    let output = body["output"].as_array().unwrap();
    assert_eq!(output.len(), 2);
    assert_eq!(output[0]["trace_summary"], "trace-a");
    assert_eq!(output[1]["memory_summary"], "memory-b");
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    for line in &seen {
        assert!(
            line.starts_with("POST https://gem.example/v1beta/models/gpt-x:generateContent"),
            "{line}"
        );
    }
    let sent = sent_body(&h, 1);
    assert_eq!(
        sent["generationConfig"]["responseMimeType"],
        "application/json"
    );
    assert!(sent.to_string().contains("/tmp/b"));
}

#[tokio::test]
async fn memory_rejection_stops_after_the_first_trace_without_partial_output() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gem", "gemini").await;
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("content-type", "application/json")],
            vec![b"{\"error\":\"rate limited\"}".as_slice().into()],
        ),
        json_reply(StatusCode::OK, gemini_text("unused".into())),
    ]);
    let (status, body, _) = run(
        &h,
        "gem",
        key(Operation::SummarizeMemory, Dialect::OpenAi),
        wire("/v1/memories/trace_summarize", memory_request()),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "rate limited");
    assert_eq!(h.client.seen.lines().len(), 1);
}
