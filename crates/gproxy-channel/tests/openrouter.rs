#![cfg(feature = "openrouter")]

//! OpenRouter's two reasons to be a channel: it routes and it prices.

use gproxy_channel::channel::{PrepareContext, QuotaScope, QuotaValue};
use gproxy_channel::channels::openrouter::{
    OpenRouter, UPSTREAM_COST_METRIC, UPSTREAM_PRICED_DIMENSION,
};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{credential, provider, quota, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = json!({"api_key": "sk-or-v1-upstream"});
    OpenRouter.prepare(PrepareContext {
        provider: provider("openrouter", config, base_url),
        credential: credential("api_key", &secret, &Value::Null),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: None,
    })
}

fn body_of(prepared: http::Request<HttpBody>) -> Value {
    let HttpBody::Bytes(bytes) = prepared.into_body() else {
        panic!("a buffered body");
    };
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn the_default_origin_carries_api_and_images_share_one_path() {
    let prepared = prepare(
        &json!({}),
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", Some("key=leak&stream=true")),
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://openrouter.ai/api/v1/chat/completions?stream=true",
        "the origin already ends in /api"
    );
    assert_eq!(
        prepared.headers()["authorization"],
        "Bearer sk-or-v1-upstream"
    );
    assert!(prepared.headers().get("host").is_none());

    for operation in [Operation::CreateImage, Operation::EditImage] {
        let prepared = prepare(
            &json!({}),
            None,
            operation,
            Dialect::OpenAi,
            request("/v1/images/generations", None),
        )
        .unwrap();
        assert_eq!(prepared.uri(), "https://openrouter.ai/api/v1/images");
    }
}

#[test]
fn attribution_is_the_callers_to_state_and_configuration_only_fills_the_gap() {
    let config = json!({"referer": "https://configured.example", "title": "Configured"});
    let prepared = prepare(
        &config,
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["http-referer"],
        "https://configured.example"
    );
    assert_eq!(prepared.headers()["x-title"], "Configured");

    let mut claimed = request("/v1/chat/completions", None);
    claimed
        .headers
        .insert("x-title", HeaderValue::from_static("The Caller"));
    let prepared = prepare(
        &config,
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        claimed,
    )
    .unwrap();
    assert_eq!(prepared.headers()["x-title"], "The Caller");

    // An allow-list narrows other client headers but cannot silence the app
    // identity OpenRouter ranks by.
    let narrowed = json!({"allowed_headers": ["x-nothing"], "title": "Configured"});
    let mut claimed = request("/v1/chat/completions", None);
    claimed
        .headers
        .insert("http-referer", HeaderValue::from_static("https://app"));
    let prepared = prepare(
        &narrowed,
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        claimed,
    )
    .unwrap();
    assert_eq!(prepared.headers()["http-referer"], "https://app");
    assert!(prepared.headers().get("anthropic-beta").is_none());
}

#[test]
fn routing_preferences_are_filled_per_model_and_never_over_the_client() {
    let config = json!({
        "provider": {"sort": "throughput", "allow_fallbacks": false},
        "model_providers": {"anthropic/claude-sonnet-4": {"only": ["anthropic"]}},
    });
    let with_body = |model: &str, extra: Value| {
        let mut request = request("/v1/chat/completions", None);
        let mut value = json!({"model": model});
        for (name, field) in extra.as_object().into_iter().flatten() {
            value[name.as_str()] = field.clone();
        }
        request.body = HttpBody::Bytes(Bytes::from(value.to_string()));
        body_of(
            prepare(
                &config,
                None,
                Operation::GenerateContent,
                Dialect::OpenAiChat,
                request,
            )
            .unwrap(),
        )
    };
    assert_eq!(
        with_body("openai/gpt-5", json!({}))["provider"],
        json!({"sort": "throughput", "allow_fallbacks": false})
    );
    assert_eq!(
        with_body("anthropic/claude-sonnet-4", json!({}))["provider"],
        json!({"only": ["anthropic"]}),
        "the per-model entry replaces the default whole"
    );
    assert_eq!(
        with_body("openai/gpt-5", json!({"provider": {"order": ["azure"]}}))["provider"],
        json!({"order": ["azure"]}),
        "a client that chose is not second-guessed"
    );
    assert_eq!(
        with_body("openai/gpt-5", json!({}))["usage"],
        json!({"include": true}),
        "usage accounting is what makes the reply say its price"
    );
}

#[test]
fn a_reported_price_becomes_the_upstream_cost_metric() {
    let body = json!({"usage": {
        "prompt_tokens": 100, "completion_tokens": 20,
        "prompt_tokens_details": {"cached_tokens": 30},
        "cost": 0.0125,
        "cost_details": {"upstream_inference_cost": 0.011},
        "is_byok": true,
    }})
    .to_string();
    let headers = HeaderMap::new();
    let usage = support::settled(
        &OpenRouter,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        &headers,
        body.as_bytes(),
    )
    .unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(70));
    assert_eq!(usage.tokens.cached_input_tokens, Some(30));
    assert_eq!(
        usage.metrics.get(UPSTREAM_COST_METRIC),
        Some(&"0.0125".parse().unwrap())
    );
    assert_eq!(
        usage.metrics.get("upstream_inference_cost_usd"),
        Some(&"0.011".parse().unwrap())
    );
    assert_eq!(
        usage
            .dimensions
            .get(UPSTREAM_PRICED_DIMENSION)
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        usage.dimensions.get("is_byok").map(String::as_str),
        Some("true")
    );
}

#[test]
fn a_stream_settles_on_the_last_chunk_that_carries_usage() {
    let wire = [
        r#"data: {"choices":[{"delta":{"content":"hi"}}],"usage":null}"#,
        r#"data: {"model":"openai/gpt-5","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":2,"cost":0.5}}"#,
        "data: [DONE]",
    ]
    .map(|chunk| format!("{chunk}\n\n"))
    .concat();
    let usage = support::settled_stream(
        &OpenRouter,
        Operation::StreamGenerateContent,
        Dialect::OpenAiChat,
        &HeaderMap::new(),
        wire.as_bytes(),
    )
    .unwrap();
    assert_eq!(usage.dimensions["serving_model"], "openai/gpt-5");
    assert_eq!(usage.tokens.input_tokens, Some(10));
    assert_eq!(
        usage.metrics.get(UPSTREAM_COST_METRIC),
        Some(&"0.5".parse().unwrap())
    );
}

#[tokio::test]
async fn the_key_probe_reports_a_budget_and_the_rate_limit_beside_it() {
    let snapshot = quota(
        &OpenRouter,
        "openrouter",
        &json!({"api_key": "sk-or"}),
        None,
        StatusCode::OK,
        json!({"data": {
            "label": "prod key", "usage": 1.5, "limit": 10.0,
            "rate_limit": {"requests": 200, "interval": "10s"},
        }})
        .to_string(),
    )
    .await
    .unwrap();
    let requested = snapshot.0;
    assert_eq!(requested, "https://openrouter.ai/api/v1/auth/key");
    let entries = snapshot.1.entries;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].model_scope, QuotaScope::All);
    let QuotaValue::Budget(budget) = &entries[0].value else {
        panic!("a budget");
    };
    assert_eq!(budget.used, Some("1.5".parse().unwrap()));
    assert_eq!(budget.remaining, Some("8.5".parse().unwrap()));
    assert_eq!(budget.unlimited, Some(false));
    let QuotaValue::RateLimit(rate) = &entries[1].value else {
        panic!("a rate limit");
    };
    assert_eq!(rate.limit, Some(200.into()));
    assert_eq!(entries[1].label.as_deref(), Some("requests per 10s"));
    use gproxy_channel::channel::QuotaModel;
    let config = json!({});
    let secret = json!({"api_key": "sk-or"});
    let declared = OpenRouter.dimensions(
        provider("openrouter", &config, None),
        credential("api_key", &secret, &Value::Null),
    );
    // The request rate limit is observed beside the budget, never charged.
    support::assert_quota_contract(
        Some(&OpenRouter),
        &declared,
        &entries,
        &["openrouter_key_rate_limit"],
    );

    let unlimited = quota(
        &OpenRouter,
        "openrouter",
        &json!({"api_key": "sk-or"}),
        None,
        StatusCode::OK,
        json!({"data": {"usage": 3.0, "limit": null}}).to_string(),
    )
    .await
    .unwrap()
    .1;
    let QuotaValue::Budget(budget) = &unlimited.entries[0].value else {
        panic!("a budget");
    };
    assert_eq!(
        budget.unlimited,
        Some(true),
        "a key with no ceiling spends the account balance"
    );

    let refused = quota(
        &OpenRouter,
        "openrouter",
        &json!({"api_key": "sk-or"}),
        None,
        StatusCode::UNAUTHORIZED,
        "{}".into(),
    )
    .await;
    assert!(matches!(
        refused,
        Err(ChannelError::UpstreamResponse { status, .. }) if status == StatusCode::UNAUTHORIZED
    ));
}

#[test]
fn the_descriptor_names_the_keys_the_channel_decodes() {
    let descriptor = OpenRouter.descriptor();
    assert_eq!(descriptor.id, "openrouter");
    assert_eq!(descriptor.login_modes, [LoginMode::ApiKey]);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.refresh);
    for key in ["provider", "model_providers", "usage_accounting", "referer"] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
    assert_eq!(
        OpenRouter.native_dialects(
            provider("openrouter", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAi, Dialect::OpenAiChat, Dialect::Claude]
    );
}

#[test]
fn a_credential_without_a_key_is_refused_before_any_url_is_built() {
    let secret = json!({});
    let error = OpenRouter.prepare(PrepareContext {
        provider: provider("openrouter", &json!({}), None),
        credential: credential("api_key", &secret, &Value::Null),
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        request: request("/v1/chat/completions", None),
        endpoint_override: None,
    });
    assert!(matches!(error, Err(ChannelError::InvalidCredential)));
}

#[test]
fn a_streamed_or_unparseable_body_is_forwarded_byte_for_byte() {
    let mut streamed = request("/v1/chat/completions", None);
    streamed.body = HttpBody::Stream(Box::pin(futures_util::stream::empty()));
    let prepared = prepare(
        &json!({"provider": {"sort": "price"}}),
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        streamed,
    )
    .unwrap();
    assert!(matches!(prepared.into_body(), HttpBody::Stream(_)));

    let mut opaque = request("/v1/audio/speech", None);
    opaque.body = HttpBody::Bytes(Bytes::from_static(b"not json"));
    let prepared = prepare(
        &json!({}),
        None,
        Operation::CreateSpeech,
        Dialect::OpenAi,
        opaque,
    )
    .unwrap();
    let HttpBody::Bytes(bytes) = prepared.into_body() else {
        panic!("buffered");
    };
    assert_eq!(bytes, Bytes::from_static(b"not json"));
}

#[test]
fn a_configured_base_url_replaces_the_default_origin() {
    let prepared = prepare(
        &json!({"headers": {"x-gateway": "on"}}),
        Some("https://mirror.example/api/"),
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_eq!(prepared.uri(), "https://mirror.example/api/v1/messages");
    assert_eq!(prepared.headers()["x-gateway"], "on");
    assert_eq!(
        prepared.headers()["authorization"],
        "Bearer sk-or-v1-upstream",
        "OpenRouter takes a bearer token on every wire, Claude included"
    );
}

#[test]
fn model_fallbacks_use_the_native_dialect_and_keep_client_routing() {
    let config = json!({"fallback_mode":"models","fallback_models":[
        "claude-opus-4-8", "claude-opus-4-8", "anthropic/claude-fable-5",
        "google/gemini-3-pro", "openai/gpt-5", "extra"
    ]});
    for (dialect, path) in [
        (Dialect::Claude, "/v1/messages"),
        (Dialect::OpenAiChat, "/v1/chat/completions"),
    ] {
        let mut wire = request(path, None);
        wire.body = HttpBody::Bytes(Bytes::from(
            json!({"model":"anthropic/claude-fable-5","messages":[]}).to_string(),
        ));
        let prepared = prepare(
            &config,
            None,
            Operation::StreamGenerateContent,
            dialect,
            wire,
        )
        .unwrap();
        assert!(
            !prepared
                .headers()
                .get("anthropic-beta")
                .is_some_and(|v| v.to_str().unwrap().contains("server-side-fallback"))
        );
        let body = body_of(prepared);
        if dialect == Dialect::Claude {
            assert_eq!(
                body["fallbacks"],
                json!([
                    {"model":"anthropic/claude-opus-4-8"},{"model":"google/gemini-3-pro"},{"model":"openai/gpt-5"}
                ])
            );
            assert!(body.get("models").is_none());
        } else {
            assert_eq!(
                body["models"],
                json!([
                    "anthropic/claude-opus-4-8",
                    "google/gemini-3-pro",
                    "openai/gpt-5"
                ])
            );
            assert!(body.get("fallbacks").is_none());
        }
        assert!(
            body.get("provider").is_none(),
            "model fallback must not configure supplier routing"
        );
        for existing in [
            json!({"models":["client-model"]}),
            json!({"fallbacks":[{"model":"client-model"}]}),
        ] {
            let mut body = json!({"model":"anthropic/claude-fable-5","messages":[]});
            body.as_object_mut()
                .unwrap()
                .extend(existing.as_object().unwrap().clone());
            let mut wire = request(path, None);
            wire.body = HttpBody::Bytes(Bytes::from(body.to_string()));
            let shaped =
                body_of(prepare(&config, None, Operation::GenerateContent, dialect, wire).unwrap());
            for name in ["models", "fallbacks"] {
                assert_eq!(shaped.get(name), body.get(name));
            }
        }
    }
}

#[test]
fn fallback_usage_is_attributed_to_the_serving_model() {
    let body = json!({"model":"anthropic/claude-opus-4-8","usage":{"prompt_tokens":10,"completion_tokens":4}});
    let usage = support::settled(
        &OpenRouter,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        &HeaderMap::new(),
        body.to_string().as_bytes(),
    )
    .unwrap();
    assert_eq!(usage.attempts[0].model, "anthropic/claude-opus-4-8");
    assert_eq!(usage.attempts[0].usage.tokens.output_tokens, Some(4));
}
