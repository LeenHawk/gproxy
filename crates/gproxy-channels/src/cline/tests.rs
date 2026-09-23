use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseShapeCtx, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};

use super::ClineChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn resolves_default_base_and_exact_override_urls() {
    let key = content(Operation::GenerateContent, Kind::OpenAiChat);
    let body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let defaults = json!({});
    let manual = prepare(
        key,
        "anthropic/claude",
        &body,
        &json!({"api_key":"manual"}),
        &defaults,
    );
    assert_eq!(
        manual.request.uri(),
        "https://api.cline.bot/api/v1/chat/completions"
    );
    assert_eq!(manual.request.method(), Method::POST);
    assert_eq!(manual.request.headers()["authorization"], "Bearer manual");
    assert_eq!(manual.request.headers()["accept"], "text/event-stream");

    let settings = json!({"base_url":"https://staging.cline.test/api/v1"});
    let account = prepare(
        key,
        "openai/gpt",
        &body,
        &json!({"access_token":"account-jwt","refresh_token":"refresh"}),
        &settings,
    );
    assert_eq!(
        account.request.uri(),
        "https://staging.cline.test/api/v1/chat/completions"
    );
    assert_eq!(
        account.request.headers()["authorization"],
        "Bearer workos:account-jwt"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_list_models":"https://models.example/catalog"}
    });
    let list = prepare(
        family(Operation::ListModels, WireFamily::OpenAi),
        "",
        &Bytes::new(),
        &json!({"api_key":"manual"}),
        &settings,
    );
    assert_eq!(list.request.uri(), "https://models.example/catalog");
    assert_eq!(list.request.method(), Method::GET);
}

#[test]
fn shapes_catalog_and_generation_envelopes_without_defaults() {
    let list_key = family(Operation::ListModels, WireFamily::OpenAi);
    let raw = Bytes::from_static(
        br#"{"free":[{"id":"a/model"}],"clinePass":[{"id":"a/model"},{"id":"b/model"}]}"#,
    );
    let catalog = ClineChannel
        .shape_response(ResponseShapeCtx {
            key: list_key,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            body: &raw,
        })
        .unwrap();
    let catalog: Value = serde_json::from_slice(&catalog).unwrap();
    assert_eq!(catalog["data"].as_array().unwrap().len(), 2);
    assert_eq!(catalog["data"][0]["cline_group"], "free");
    assert!(catalog["data"][0].get("created").is_none());

    let key = content(Operation::GenerateContent, Kind::OpenAiChat);
    let raw = Bytes::from_static(
        br#"{"success":true,"data":{"id":"gen-1","choices":[],"usage":{"prompt_tokens":10,"completion_tokens":4,"total_tokens":14}}}"#,
    );
    let headers = HeaderMap::new();
    let usage = ClineChannel
        .extract_usage(UsageCtx {
            key,
            request_body: &Bytes::new(),
            response_headers: &headers,
            response_body: &raw,
        })
        .unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (10, 4));
    let outward = ClineChannel
        .shape_response(ResponseShapeCtx {
            key,
            status: StatusCode::OK,
            headers: &headers,
            body: &raw,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    assert_eq!(outward["id"], "gen-1");
    assert!(outward.get("data").is_none());
    assert!(outward.get("success").is_none());
}

fn prepare(
    key: OperationKey,
    model: &str,
    body: &Bytes,
    secret: &Value,
    settings: &Value,
) -> gproxy_channel_api::PreparedRequest {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    ClineChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: key.operation() == Operation::StreamGenerateContent,
            method: &Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers: &headers,
            body,
            upstream_model: model,
            provider_settings: settings,
            secret,
        })
        .unwrap()
}

#[test]
fn refresh_preserves_manual_keys_and_repairs_legacy_login_copies() {
    use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
    struct RefreshHttp;
    impl SimpleHttp for RefreshHttp {
        fn send<'a>(
            &'a self,
            request: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<Bytes>, ChannelError>> {
            assert_eq!(request.uri(), "https://api.cline.bot/api/v1/auth/refresh");
            assert_eq!(
                serde_json::from_slice::<Value>(request.body()).unwrap()["refreshToken"],
                "refresh"
            );
            Box::pin(async {
                Ok(http::Response::new(Bytes::from_static(br#"{"success":true,"data":{"accessToken":"new-login","refreshToken":"new-refresh"}}"#)))
            })
        }
    }
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJleHAiOjF9.signature";
    for (secret, expected) in [
        (
            json!({"api_key":"sk-manual","access_token":"old-login","refresh_token":"refresh"}),
            "Bearer sk-manual",
        ),
        (
            json!({"access_token":"old-login","refresh_token":"refresh"}),
            "Bearer workos:new-login",
        ),
        (
            json!({"api_key":"old-login","access_token":"old-login","refresh_token":"refresh"}),
            "Bearer workos:new-login",
        ),
        (
            json!({"api_key":jwt,"access_token":"newer-login","refresh_token":"refresh"}),
            "Bearer workos:new-login",
        ),
    ] {
        let settings = json!({});
        let mut future = ClineChannel
            .refresh(&secret, &settings, &RefreshHttp)
            .unwrap();
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let std::task::Poll::Ready(result) = future.as_mut().poll(&mut context) else {
            panic!("mock must be ready")
        };
        let rotated = result.unwrap();
        assert_eq!(rotated["access_token"], "new-login");
        assert_eq!(rotated["refresh_token"], "new-refresh");
        if expected == "Bearer sk-manual" {
            assert_eq!(rotated["api_key"], "sk-manual");
        } else {
            assert!(rotated.get("api_key").is_none());
        }
        // Exercise the persisted JSON representation used by subsequent quota requests.
        let stored: Value = serde_json::from_slice(&serde_json::to_vec(&rotated).unwrap()).unwrap();
        let request = ClineChannel
            .prepare_quota_source("plan_usage", &stored, &settings)
            .unwrap()
            .unwrap();
        assert_eq!(request.headers()["authorization"], expected);
    }
    assert!(
        ClineChannel
            .refresh(&json!({"api_key":"sk-manual"}), &json!({}), &RefreshHttp)
            .is_none()
    );
}

#[test]
fn legacy_login_keys_use_workos_authorization_before_refresh() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJleHAiOjF9.signature";
    for (secret, expected) in [
        (json!({"api_key":jwt}), format!("Bearer workos:{jwt}")),
        (
            json!({"api_key":format!("workos:{jwt}")}),
            format!("Bearer workos:{jwt}"),
        ),
        (
            json!({"api_key":"login","access_token":"login"}),
            "Bearer workos:login".into(),
        ),
        (
            json!({"api_key":jwt,"access_token":"newer-login"}),
            "Bearer workos:newer-login".into(),
        ),
    ] {
        let request = ClineChannel
            .prepare_quota_source("plan_usage", &secret, &json!({}))
            .unwrap()
            .unwrap();
        assert_eq!(request.headers()["authorization"], expected);
        assert_eq!(
            super::auth::bearer(&secret).unwrap(),
            expected.strip_prefix("Bearer ").unwrap()
        );
    }
}
