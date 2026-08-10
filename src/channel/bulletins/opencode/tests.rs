//! The failure-prone part of this channel is that both the upstream path and the
//! credential header come from the ROUTED cell rather than the inbound path —
//! including the Gemini model-in-path verb. These cover that mapping, the one
//! cell where the two tiers differ, and the two login decisions that would
//! silently break a pasted API key.

use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::*;
use crate::channel::routes::RoutingDecision;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

fn ctx<'a>(
    settings: &'a Value,
    secret: &'a Value,
    headers: &'a HeaderMap,
    kind: ContentGenerationKind,
    stream: bool,
) -> PrepareCtx<'a> {
    PrepareCtx {
        secret,
        provider_settings: settings,
        op: OperationKey::content_generation(
            if stream {
                Operation::StreamGenerateContent
            } else {
                Operation::GenerateContent
            },
            kind,
        ),
        stream,
        upstream_model_id: "gemini-3-flash",
        method: Method::POST,
        // Deliberately unrelated to the routed cell: a transformed candidate
        // arrives with the downstream client's original path.
        path: "/v1/chat/completions",
        query: None,
        headers,
        body: Bytes::from_static(b"{}"),
    }
}

#[test]
fn zen_routes_each_surface_to_its_path_and_auth_header() {
    use ContentGenerationKind::*;
    let settings = json!({});
    let secret = json!({ "api_key": "oc-key" });
    let h = HeaderMap::new();

    for (kind, stream, path, header, value) in [
        (
            OpenAiChatCompletions,
            false,
            "https://opencode.ai/zen/v1/chat/completions",
            "authorization",
            "Bearer oc-key",
        ),
        (
            OpenAiResponses,
            false,
            "https://opencode.ai/zen/v1/responses",
            "authorization",
            "Bearer oc-key",
        ),
        (
            ClaudeMessages,
            false,
            "https://opencode.ai/zen/v1/messages",
            "x-api-key",
            "oc-key",
        ),
        (
            GeminiGenerateContent,
            false,
            "https://opencode.ai/zen/v1/models/gemini-3-flash:generateContent",
            "x-goog-api-key",
            "oc-key",
        ),
        (
            GeminiGenerateContent,
            true,
            "https://opencode.ai/zen/v1/models/gemini-3-flash:streamGenerateContent",
            "x-goog-api-key",
            "oc-key",
        ),
    ] {
        let req = OpenCodeZenChannel
            .prepare(ctx(&settings, &secret, &h, kind, stream))
            .unwrap()
            .into_http()
            .unwrap();
        assert_eq!(req.uri().to_string(), path, "{kind:?} stream={stream}");
        assert_eq!(req.headers().get(header).unwrap(), value, "{kind:?}");
    }

    // The Claude surface is the real Anthropic wire format upstream.
    let req = OpenCodeZenChannel
        .prepare(ctx(&settings, &secret, &h, ClaudeMessages, false))
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(
        req.headers().get("anthropic-version").unwrap(),
        "2023-06-01"
    );
}

#[test]
fn go_uses_the_go_base_and_has_no_native_gemini_cell() {
    let settings = json!({});
    let secret = json!({ "api_key": "oc-key" });
    let h = HeaderMap::new();
    let req = OpenCodeGoChannel
        .prepare(ctx(
            &settings,
            &secret,
            &h,
            ContentGenerationKind::OpenAiChatCompletions,
            true,
        ))
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(
        req.uri().to_string(),
        "https://opencode.ai/zen/go/v1/chat/completions"
    );

    let gemini = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    );
    let decision = OpenCodeGoChannel
        .routing_table()
        .into_iter()
        .find(|(key, _)| *key == gemini)
        .map(|(_, decision)| decision);
    assert!(
        matches!(decision, Some(RoutingDecision::TransformTo(_))),
        "Go has no /models/{{model}} route; Gemini must convert, got {decision:?}"
    );
    assert!(
        OpenCodeZenChannel
            .routing_table()
            .contains(&(gemini, RoutingDecision::Passthrough)),
        "Zen serves Gemini natively"
    );
}

#[test]
fn model_list_hits_the_gateway_catalogue() {
    let settings = json!({});
    let secret = json!({ "api_key": "oc-key" });
    let headers = HeaderMap::new();
    let req = OpenCodeZenChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::provider(Operation::ListModels, crate::protocol::Provider::OpenAi),
            stream: false,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(req.method(), Method::GET);
    assert_eq!(req.uri().to_string(), "https://opencode.ai/zen/v1/models");
    assert_eq!(req.headers().get("authorization").unwrap(), "Bearer oc-key");
}

/// A pasted API key carries no console tokens. If that were treated as
/// refreshable, every manual credential would fail on its first request.
#[test]
fn only_console_minted_credentials_refresh() {
    assert!(!OpenCodeZenChannel.needs_refresh(&json!({ "api_key": "oc-key" })));
    assert!(!OpenCodeGoChannel.needs_refresh(&json!({ "api_key": "oc-key" })));
    // Console-minted, still valid for another hour.
    let fresh_ms = crate::util::time::unix_now().saturating_mul(1000) + 3_600_000;
    assert!(!OpenCodeZenChannel.needs_refresh(&json!({
        "api_key": "oc-key",
        "refresh_token": "rt",
        "expires_at_ms": fresh_ms,
    })));
    assert!(OpenCodeZenChannel.needs_refresh(&json!({
        "api_key": "oc-key",
        "refresh_token": "rt",
        "expires_at_ms": 0,
    })));
}
