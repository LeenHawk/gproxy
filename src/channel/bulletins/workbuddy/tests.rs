use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::*;

#[test]
fn prepares_chat_with_account_identity() {
    let request = WorkBuddyChannel
        .prepare(PrepareCtx {
            secret: &json!({
                "access_token": "token",
                "user_id": "user-1",
                "enterprise_id": "ent-1",
                "department_full_name": "Engineering",
                "domain": "copilot.tencent.com"
            }),
            provider_settings: &json!({}),
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::OpenAiChatCompletions,
            ),
            stream: false,
            upstream_model_id: "default",
            method: Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &HeaderMap::new(),
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(
        request.uri(),
        "https://copilot.tencent.com/v2/chat/completions"
    );
    assert_eq!(request.headers()["authorization"], "Bearer token");
    assert_eq!(request.headers()["x-user-id"], "user-1");
    assert_eq!(request.headers()["x-enterprise-id"], "ent-1");
}

#[test]
fn unwraps_image_response() {
    let body = Bytes::from_static(br#"{"code":0,"data":{"data":[{"b64_json":"abc"}]}}"#);
    let shaped = shape::response(
        body,
        &ShapeCtx {
            op: OperationKey::provider(Operation::CreateImage, crate::protocol::Provider::OpenAi),
            stream: false,
            settings: &Value::Null,
            status: StatusCode::OK,
        },
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["data"][0]["b64_json"], "abc");
}

#[test]
fn parses_personal_usage_resources() {
    let body = Bytes::from_static(
        br#"{"data":{"Response":{"Data":{"Accounts":[{"PackageCode":"free","CycleCapacitySizePrecise":"100","CycleCapacityRemainPrecise":"35","CycleEndTime":"2026-08-13T00:00:00Z"}]}}}}"#,
    );
    let snapshot = usage::parse(StatusCode::OK, &body).unwrap();
    assert_eq!(snapshot.windows[0].used, Some(65.0));
    assert_eq!(snapshot.windows[0].limit, Some(100.0));
}
