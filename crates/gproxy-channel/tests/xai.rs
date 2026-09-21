#![cfg(feature = "xai")]

//! xAI sits on OpenAI's shapes but keeps its own media paths, its own
//! metering unit and its billing on another host entirely.

use gproxy_channel::channel::{
    CredentialContext, PrepareContext, QuotaModel, QuotaQuery, QuotaValue, ResponseView,
    UsageContext, UsageExtractor,
};
use gproxy_channel::channels::xai::{
    COST_TICKS_METRIC, POSTPAID_DIMENSION, PREPAID_DIMENSION, UPSTREAM_COST_METRIC,
    UPSTREAM_PRICED_DIMENSION, Xai,
};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{OneShot, credential, provider, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = json!({"api_key": "xai-upstream"});
    Xai.prepare(PrepareContext {
        provider: provider("xai", config, base_url),
        credential: credential("api_key", &secret, &Value::Null),
        operation: OperationKey {
            operation,
            dialect: Dialect::OpenAi,
        },
        request,
        endpoint_override: None,
    })
}

#[test]
fn media_operations_take_xais_paths_and_everything_else_keeps_its_own() {
    for (operation, path, expected) in [
        (
            Operation::CreateSpeech,
            "/v1/audio/speech",
            "https://api.x.ai/v1/tts",
        ),
        (
            Operation::CreateTranscription,
            "/v1/audio/transcriptions",
            "https://api.x.ai/v1/stt",
        ),
        (
            Operation::CreateVideo,
            "/v1/videos",
            "https://api.x.ai/v1/videos/generations",
        ),
        (
            Operation::GenerateContent,
            "/v1/responses",
            "https://api.x.ai/v1/responses",
        ),
        (
            Operation::CreateImage,
            "/v1/images/generations",
            "https://api.x.ai/v1/images/generations",
        ),
    ] {
        let mut caller = request(path, None);
        if operation == Operation::CreateSpeech {
            caller.body = HttpBody::Bytes(Bytes::from_static(br#"{"input":"hi"}"#));
        }
        let prepared = prepare(&json!({}), None, operation, caller).unwrap();
        assert_eq!(prepared.uri(), expected, "{operation:?}");
        assert_eq!(prepared.headers()["authorization"], "Bearer xai-upstream");
        assert!(prepared.headers().get("x-api-key").is_none());
    }
}

#[test]
fn a_speech_body_is_rewritten_into_xais_own_fields() {
    let mut caller = request("/v1/audio/speech", None);
    caller.body = HttpBody::Bytes(Bytes::from(
        json!({"model": "grok-voice", "input": "hello", "voice": "ara",
               "response_format": "mp3", "instructions": "cheerful", "speed": 1.2})
        .to_string(),
    ));
    let prepared = prepare(&json!({}), None, Operation::CreateSpeech, caller).unwrap();
    let HttpBody::Bytes(bytes) = prepared.into_body() else {
        panic!("buffered");
    };
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"text": "hello", "voice_id": "ara",
               "output_format": {"codec": "mp3"}, "speed": 1.2}),
        "model, instructions and stream_format have no counterpart"
    );

    let mut streamed = request("/v1/audio/speech", None);
    streamed.body = HttpBody::Stream(Box::pin(futures_util::stream::empty()));
    assert!(
        matches!(
            prepare(&json!({}), None, Operation::CreateSpeech, streamed),
            Err(ChannelError::InvalidConfig(_))
        ),
        "the rename needs the body in hand"
    );
}

#[test]
fn the_grok_conversation_header_survives_a_narrow_allow_list() {
    let mut caller = request("/v1/chat/completions", Some("api_key=leak&n=1"));
    caller
        .headers
        .insert("x-grok-conv-id", HeaderValue::from_static("conv-1"));
    let prepared = prepare(
        &json!({"allowed_headers": ["x-nothing"], "headers": {"x-vendor": "on"}}),
        Some("https://mirror.example/"),
        Operation::GenerateContent,
        caller,
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://mirror.example/v1/chat/completions?n=1"
    );
    assert_eq!(prepared.headers()["x-grok-conv-id"], "conv-1");
    assert_eq!(prepared.headers()["x-vendor"], "on");
    assert!(prepared.headers().get("anthropic-beta").is_none());
}

fn usage_of(body: Value) -> gproxy_channel::channel::NormalizedUsage {
    let text = body.to_string();
    let headers = HeaderMap::new();
    Xai.extract(UsageContext {
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAi,
        },
        request_body: None,
        response: ResponseView {
            status: StatusCode::OK,
            headers: &headers,
            body: text.as_bytes(),
        },
    })
    .unwrap()
    .unwrap()
}

#[test]
fn cost_ticks_are_a_metric_of_their_own_and_a_video_job_states_dollars() {
    let usage = usage_of(json!({"usage": {
        "input_tokens": 100, "output_tokens": 5,
        "input_tokens_details": {"cached_tokens": 40, "image_tokens": 12},
        "cost_in_usd_ticks": 1234,
        "server_side_tool_usage_details": {"web_search_requests": 2},
    }}));
    assert_eq!(usage.tokens.input_tokens, Some(60));
    assert_eq!(usage.tokens.cached_input_tokens, Some(40));
    assert_eq!(
        usage.metrics.get(COST_TICKS_METRIC),
        Some(&1234.into()),
        "ticks are xAI's unit, not dollars"
    );
    assert_eq!(usage.metrics.get("image_input_tokens"), Some(&12.into()));
    assert_eq!(usage.metrics.get("web_searches"), Some(&2.into()));
    assert!(
        !usage.dimensions.contains_key(UPSTREAM_PRICED_DIMENSION),
        "ticks alone do not price the exchange"
    );

    let video = usage_of(json!({
        "usage": {"input_tokens": 4, "output_tokens": 0},
        "cost_usd": 0.4, "duration": 6,
    }));
    assert_eq!(
        video.metrics.get(UPSTREAM_COST_METRIC),
        Some(&"0.4".parse().unwrap())
    );
    assert_eq!(
        video
            .dimensions
            .get(UPSTREAM_PRICED_DIMENSION)
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(video.metrics.get("video_seconds"), Some(&6.into()));
}

#[tokio::test]
async fn billing_is_a_second_host_behind_a_second_key() {
    let config = json!({"quota_api_key": "mgmt-key", "quota_team_id": "team-7"});
    let secret = json!({"api_key": "xai-inference"});
    // Both calls read the same scripted document; each parser takes its own
    // field out of it.
    let client = OneShot::new(
        StatusCode::OK,
        json!({
            "total": {"val": -12_345},
            "spendingLimits": {"effectiveSl": {"val": 50_000}},
        })
        .to_string(),
    );
    let snapshot = Xai
        .query(CredentialContext {
            provider: provider("xai", &config, None),
            credential: credential("api_key", &secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(
        client.call(0).0,
        "https://management-api.x.ai/v1/billing/teams/team-7/prepaid/balance"
    );
    assert_eq!(
        client.call(1).0,
        "https://management-api.x.ai/v1/billing/teams/team-7/postpaid/spending-limits"
    );
    assert_eq!(
        client.call(0).1["authorization"],
        "Bearer mgmt-key",
        "the inference key has no billing surface"
    );
    let QuotaValue::Balance(prepaid) = &snapshot.entries[0].value else {
        panic!("a balance");
    };
    assert_eq!(
        prepaid.remaining,
        Some("123.45".parse().unwrap()),
        "signed cents, purchases negative"
    );
    let QuotaValue::Budget(postpaid) = &snapshot.entries[1].value else {
        panic!("a budget");
    };
    assert_eq!(postpaid.limit, Some("500".parse().unwrap()));
}

#[test]
fn without_a_management_key_no_dimension_is_declared_and_no_probe_is_made() {
    let secret = json!({"api_key": "xai"});
    assert!(
        Xai.dimensions(
            provider("xai", &json!({}), None),
            credential("api_key", &secret, &Value::Null)
        )
        .is_empty(),
        "a dimension that can never be filled is not declared"
    );
    let configured = json!({"quota_api_key": "k", "quota_team_id": "t"});
    let declared = Xai.dimensions(
        provider("xai", &configured, None),
        credential("api_key", &secret, &Value::Null),
    );
    assert_eq!(
        declared.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        [PREPAID_DIMENSION, POSTPAID_DIMENSION]
    );
}

#[test]
fn the_descriptor_names_the_billing_keys_and_the_two_native_shapes() {
    let descriptor = Xai.descriptor();
    assert_eq!(descriptor.id, "xai");
    assert_eq!(descriptor.login_modes, [LoginMode::ApiKey]);
    assert!(descriptor.capabilities.quota_query);
    for key in ["quota_api_key", "quota_team_id", "quota_base_url"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
    assert_eq!(
        Xai.native_dialects(
            provider("xai", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi],
        "Claude and Gemini callers reach Grok through conversion"
    );
}
