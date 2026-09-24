#![cfg(not(target_arch = "wasm32"))]
mod support;
use support::*;

use gproxy_channel::{
    channel::ProviderView,
    channels::{codex::Codex, openai::OpenAi},
};
use gproxy_core::convert::{Route, default_route};
use gproxy_protocol::{
    Dialect, Operation, OperationKey,
    connection::Bytes,
    transform::{
        generate::stream::responses::synthesize_responses_stream,
        identity::{IdNamespace, IdentityFlow},
    },
};
use gproxy_store::entity::{config::setting, upstream::provider};
use http::StatusCode;
use sea_orm::Set;
use serde_json::{Value, json};

fn key(operation: Operation, dialect: Dialect) -> OperationKey {
    OperationKey { operation, dialect }
}

#[test]
fn codex_defaults_use_streaming_responses_without_changing_openai_defaults() {
    let config = json!({});
    let view = ProviderView {
        id: "p",
        channel: "codex",
        base_url: None,
        config: &config,
    };
    for dialect in [
        Dialect::OpenAi,
        Dialect::OpenAiChat,
        Dialect::Claude,
        Dialect::Gemini,
    ] {
        assert_eq!(
            default_route(&Codex, view, key(Operation::GenerateContent, dialect)),
            Route::TransformTo {
                target: key(Operation::StreamGenerateContent, Dialect::OpenAi)
            }
        );
        assert_eq!(
            default_route(&Codex, view, key(Operation::StreamGenerateContent, dialect)),
            if dialect == Dialect::OpenAi {
                Route::Passthrough
            } else {
                Route::TransformTo {
                    target: key(Operation::StreamGenerateContent, Dialect::OpenAi),
                }
            }
        );
        assert_ne!(
            default_route(&OpenAi, view, key(Operation::GenerateContent, dialect)),
            Route::Unsupported
        );
    }
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        assert_eq!(
            default_route(
                &Codex,
                view,
                key(operation, Dialect::OpenAiResponsesWebSocket)
            ),
            Route::Passthrough
        );
    }
}

fn sse() -> Vec<Bytes> {
    let body = serde_json::from_value(json!({
        "id":"resp_answer", "created_at":1, "model":"gpt-x", "object":"response", "status":"completed",
        "error":null, "incomplete_details":null, "instructions":null, "metadata":null,
        "parallel_tool_calls":true, "temperature":null, "top_p":null, "tools":[], "tool_choice":"auto",
        "output":[{"type":"message","id":"msg_answer","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hello","annotations":[],"logprobs":[]}]}],
        "usage":{"input_tokens":4,"output_tokens":2,"total_tokens":6,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}
    })).unwrap();
    synthesize_responses_stream(
        body,
        &mut IdentityFlow::new(IdNamespace::with_bytes([1; 16])),
        Default::default(),
    )
    .unwrap()
    .value
    .into_iter()
    .flat_map(|event| {
        // Split framing and JSON across transport chunks.
        let line = format!("data: {}\n\n", serde_json::to_string(&event).unwrap());
        line.as_bytes()
            .chunks(7)
            .map(Bytes::copy_from_slice)
            .collect::<Vec<_>>()
    })
    .collect()
}

async fn streaming_provider(h: &Harness) {
    seed_provider(h, "responses", "https://responses.example", "openai").await;
    h.core
        .store()
        .providers()
        .update_many(vec![provider::ActiveModel {
            id: Set("responses".into()),
            config: Set(json!({"dialects":["openai"],"streaming_only":true})),
            ..Default::default()
        }])
        .await
        .unwrap();
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set((h.core.snapshot().revision.0 + 1) as i64),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

#[tokio::test]
async fn buffered_clients_collect_responses_streams_and_preserve_rejections() {
    for (dialect, body) in [
        (
            Dialect::OpenAi,
            json!({"model":"alias","input":"hi","stream":false}),
        ),
        (
            Dialect::OpenAiChat,
            json!({"model":"alias","messages":[{"role":"user","content":"hi"}],"stream":false}),
        ),
        (
            Dialect::Claude,
            json!({"model":"alias","max_tokens":32,"messages":[{"role":"user","content":"hi"}],"stream":false}),
        ),
        (
            Dialect::Gemini,
            json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]}),
        ),
    ] {
        let h = harness(full(), "round_robin").await;
        streaming_provider(&h).await;
        h.script(vec![(
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            sse(),
        )]);
        let ctx = h.context_for(
            "responses",
            key(Operation::GenerateContent, dialect),
            "collect",
            1,
            None,
        );
        let (response, completion) = h
            .core
            .generate_content(ctx, request(&body.to_string()))
            .await
            .unwrap()
            .into_parts();
        assert_eq!(response.headers["content-type"], "application/json");
        let answer: Value = serde_json::from_str(&read(response.body).await).unwrap();
        let text = match dialect {
            Dialect::OpenAi => answer["output"][0]["content"][0]["text"]
                .as_str()
                .unwrap()
                .to_owned(),
            Dialect::OpenAiChat => answer["choices"][0]["message"]["content"]
                .as_str()
                .unwrap()
                .to_owned(),
            Dialect::Claude => answer["content"][0]["text"].as_str().unwrap().to_owned(),
            Dialect::Gemini => answer["candidates"][0]["content"]["parts"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<String>(),
            _ => unreachable!(),
        };
        assert_eq!(text, "hello", "{dialect:?}: {answer}");
        assert_eq!(
            completion.await.unwrap().state,
            gproxy_core::UsageState::Completed
        );
        let sent = h.client.seen.lines();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].contains("\"stream\":true"), "{}", sent[0]);

        h.script(vec![json_reply(
            StatusCode::BAD_REQUEST,
            json!({"error":{"message":"rejected"}}),
        )]);
        let ctx = h.context_for(
            "responses",
            key(Operation::GenerateContent, dialect),
            "rejected",
            1,
            None,
        );
        let (response, completion) = h
            .core
            .generate_content(ctx, request(&body.to_string()))
            .await
            .unwrap()
            .into_parts();
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(
            serde_json::from_str::<Value>(&read(response.body).await).unwrap(),
            json!({"error":{"message":"rejected"}})
        );
        completion.await.unwrap();
    }
}
