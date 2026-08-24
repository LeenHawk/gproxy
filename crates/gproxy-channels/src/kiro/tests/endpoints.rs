use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::Method;
use serde_json::{Value, json};

use super::support::prepare;

#[test]
fn builds_runtime_default_and_exact_model_override() {
    let secret = json!({
        "access_token":"access","refresh_token":"social-refresh",
        "profile_arn":"arn:aws:codewhisperer:us-east-1:1:profile/x"
    });
    let key = OperationKey::content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let body = Bytes::from_static(br#"{"model":"route","input":"hello"}"#);
    let runtime = prepare(key, "claude-sonnet-4-6", &body, &secret, &json!({}));
    assert_eq!(runtime.request.uri(), "https://runtime.us-east-1.kiro.dev/");
    assert_eq!(runtime.request.method(), Method::POST);
    assert_eq!(runtime.request.headers()["authorization"], "Bearer access");
    assert_eq!(
        runtime.request.headers()["x-amz-target"],
        "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
    );
    assert_eq!(
        runtime.profile,
        Some(&super::super::profile::CLIENT_PROFILE)
    );
    let request: Value = serde_json::from_slice(runtime.request.body()).unwrap();
    assert_eq!(request["profileArn"], secret["profile_arn"]);
    assert_eq!(
        request["conversationState"]["currentMessage"]["userInputMessage"]["modelId"],
        "claude-sonnet-4.6"
    );

    let list = prepare(
        OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
        "",
        &Bytes::new(),
        &secret,
        &json!({"endpoints":{"openai_list_models":"https://models.example/catalog"}}),
    );
    assert!(
        list.request
            .uri()
            .to_string()
            .starts_with("https://models.example/catalog?origin=KIRO_CLI&profileArn=")
    );
    assert_eq!(list.request.method(), Method::POST);
}
