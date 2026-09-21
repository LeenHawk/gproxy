#![cfg(not(target_arch = "wasm32"))]

//! Model directory and token counting conversions driven end to end through
//! Core against a Claude-only provider seeded with per-model supplements.

mod support;
use support::*;

use gproxy_core::CoreError;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use gproxy_store::entity::{config::setting, upstream::provider_model};
use http::{HeaderMap, Method, StatusCode};
use sea_orm::Set;
use serde_json::json;

/// A Claude `ModelInfo` as the upstream returns it, capabilities included.
fn claude_model(id: &str) -> serde_json::Value {
    let support = json!({"supported": false});
    json!({
        "id": id, "type": "model", "display_name": id, "created_at": "2024-01-01T00:00:00Z",
        "allowed_fallback_models": [], "max_input_tokens": 1000, "max_tokens": 100,
        "capabilities": {
            "batch": support, "citations": support, "code_execution": support,
            "image_input": support, "pdf_input": support, "structured_outputs": support,
            "context_management": {"supported": false, "clear_thinking_20251015": support,
                "clear_tool_uses_20250919": support, "compact_20260112": support},
            "effort": {"supported": false, "high": support, "low": support, "max": support,
                "medium": support, "xhigh": support},
            "thinking": {"supported": true, "types": {"adaptive": support, "enabled": support}}
        }
    })
}

/// Seed `m1` with OpenAI and Gemini supplements and `m2` with none.
async fn seeded() -> Harness {
    let h = harness(full(), "round_robin").await;
    let row = |id: &str, metadata: serde_json::Value| provider_model::ActiveModel {
        id: Set(format!("pm-{id}")),
        provider_id: Set("claude".into()),
        upstream_name: Set(id.into()),
        model_id: Set(None),
        metadata: Set(metadata),
        enabled: Set(true),
    };
    h.core
        .store()
        .provider_models()
        .create_many(vec![
            row(
                "m1",
                json!({"supplements": {
                    "openai": {"owned_by": "anthropic", "created": 1704067200},
                    "gemini": {"base_model_id": "m1", "version": "001"}
                }}),
            ),
            row("m2", json!({})),
        ])
        .await
        .unwrap();
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(2),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
    h
}

fn get(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::GET,
        path: path.into(),
        query: query.map(str::to_owned),
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}

fn post(path: &str, body: serde_json::Value) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn key(operation: Operation, dialect: Dialect) -> OperationKey {
    OperationKey { operation, dialect }
}

#[tokio::test]
async fn openai_client_lists_a_claude_directory_using_provider_supplements() {
    let h = seeded().await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"data": [claude_model("m1")], "first_id": "m1", "last_id": "m1", "has_more": false}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::ListModels, Dialect::OpenAi),
        "r1",
        1,
        None,
    );
    let execution = h
        .core
        .list_models(ctx, get("/v1/models", None))
        .await
        .unwrap();
    let (response, _) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], "m1");
    assert_eq!(body["data"][0]["object"], "model");
    assert_eq!(body["data"][0]["owned_by"], "anthropic");
    assert_eq!(body["data"][0]["created"], 1704067200);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("GET https://claude.example/v1/models auth=Bearer k1"),
        "{}",
        seen[0]
    );
}

#[tokio::test]
async fn gemini_client_lists_a_claude_directory_and_a_missing_supplement_fails() {
    let h = seeded().await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"data": [claude_model("m1")], "first_id": "m1", "last_id": "m1", "has_more": false}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::ListModels, Dialect::Gemini),
        "r1",
        1,
        None,
    );
    let (response, _) = h
        .core
        .list_models(ctx, get("/v1beta/models", None))
        .await
        .unwrap()
        .into_parts();
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["models"][0]["name"], "models/m1");
    assert_eq!(body["models"][0]["baseModelId"], "m1");
    assert_eq!(body["models"][0]["version"], "001");
    assert_eq!(body["models"][0]["inputTokenLimit"], 1000);

    // `m2` has no Gemini supplement: the directory is not invented.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"data": [claude_model("m1"), claude_model("m2")], "first_id": "m1", "last_id": "m2", "has_more": false}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::ListModels, Dialect::Gemini),
        "r2",
        1,
        None,
    );
    let Err(error) = h.core.list_models(ctx, get("/v1beta/models", None)).await else {
        panic!("missing supplement must fail");
    };
    assert!(matches!(error, CoreError::Transform(_)), "{error:?}");
}

#[tokio::test]
async fn list_models_rejects_client_pagination_without_calling_upstream() {
    let h = seeded().await;
    let ctx = h.context_for(
        "claude",
        key(Operation::ListModels, Dialect::OpenAi),
        "r1",
        1,
        None,
    );
    let Err(error) = h
        .core
        .list_models(ctx, get("/v1/models", Some("after=m0")))
        .await
    else {
        panic!("pagination cursors are not converted");
    };
    assert!(matches!(error, CoreError::Transform(_)), "{error:?}");
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn gemini_client_gets_one_claude_model_by_path() {
    let h = seeded().await;
    h.script(vec![json_reply(StatusCode::OK, claude_model("m1"))]);
    let ctx = h.context_for(
        "claude",
        key(Operation::GetModel, Dialect::Gemini),
        "r1",
        1,
        None,
    );
    let (response, _) = h
        .core
        .get_model(ctx, get("/v1beta/models/m1", None))
        .await
        .unwrap()
        .into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["name"], "models/m1");
    assert_eq!(body["displayName"], "m1");
    assert_eq!(body["thinking"], true);
    let seen = h.client.seen.lines();
    assert!(
        seen[0].starts_with("GET https://claude.example/v1/models/m1 auth=Bearer k1"),
        "{}",
        seen[0]
    );
}

#[tokio::test]
async fn get_model_upstream_rejection_is_returned_as_is() {
    let h = seeded().await;
    h.script(vec![json_reply(
        StatusCode::NOT_FOUND,
        json!({"type": "error", "error": {"type": "not_found_error", "message": "nope"}}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::GetModel, Dialect::OpenAi),
        "r1",
        1,
        None,
    );
    let (response, _) = h
        .core
        .get_model(ctx, get("/v1/models/m9", None))
        .await
        .unwrap()
        .into_parts();
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert!(read(response.body).await.contains("not_found_error"));
}

#[tokio::test]
async fn openai_client_counts_tokens_through_claude() {
    let h = seeded().await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"input_tokens": 7, "context_management": {"original_input_tokens": 7}}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::CountTokens, Dialect::OpenAi),
        "r1",
        1,
        None,
    );
    let (response, _) = h
        .core
        .count_tokens(
            ctx,
            post(
                "/v1/responses/input_tokens",
                json!({"model": "alias", "input": "hi"}),
            ),
        )
        .await
        .unwrap()
        .into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["object"], "response.input_tokens");
    assert_eq!(body["input_tokens"], 7);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/messages/count_tokens auth=Bearer k1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0].contains("\"model\":\"gpt-x\"") && seen[0].contains("\"role\":\"user\""),
        "native Claude count body: {}",
        seen[0]
    );
}

#[tokio::test]
async fn gemini_client_counts_tokens_through_claude() {
    let h = seeded().await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"input_tokens": 3, "context_management": {"original_input_tokens": 3}}),
    )]);
    let ctx = h.context_for(
        "claude",
        key(Operation::CountTokens, Dialect::Gemini),
        "r1",
        1,
        None,
    );
    let (response, _) = h
        .core
        .count_tokens(
            ctx,
            post(
                "/v1beta/models/alias:countTokens",
                json!({"contents": [{"role": "user", "parts": [{"text": "hi"}]}]}),
            ),
        )
        .await
        .unwrap()
        .into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["totalTokens"], 3);
}

#[tokio::test]
async fn chat_client_count_tokens_is_unsupported_without_an_upstream_call() {
    let h = seeded().await;
    let ctx = h.context_for(
        "claude",
        key(Operation::CountTokens, Dialect::OpenAiChat),
        "r1",
        1,
        None,
    );
    let Err(error) = h
        .core
        .count_tokens(ctx, post("/v1/chat/completions", json!({})))
        .await
    else {
        panic!("chat has no counting operation");
    };
    assert!(matches!(error, CoreError::Transform(_)), "{error:?}");
    assert!(h.client.seen.lines().is_empty());
}
