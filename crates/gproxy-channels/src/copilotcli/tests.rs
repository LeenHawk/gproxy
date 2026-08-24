use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::CopilotCliChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_truthful_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
        ChannelSupport::transform(
            family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels, WireFamily::OpenAi),
        ),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::OpenAiResponses),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::ClaudeMessages),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(content(Operation::StreamGenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
    ];
    assert_eq!(CopilotCliChannel.descriptor().supports, expected);
}

#[test]
fn resolves_account_default_and_exact_override_urls() {
    let mut business = secret();
    business["account_type"] = Value::String("business".into());
    let body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let settings = json!({});
    let chat = prepare(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        "gpt-5.4",
        &body,
        &business,
        &settings,
    );
    assert_eq!(
        chat.request.uri(),
        "https://api.business.githubcopilot.com/chat/completions"
    );
    assert_eq!(chat.request.method(), Method::POST);

    let settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{"openai_list_models":"https://models.example/catalog"}
    });
    let list = prepare(
        family(Operation::ListModels, WireFamily::OpenAi),
        "",
        &Bytes::new(),
        &secret(),
        &settings,
    );
    assert_eq!(list.request.uri(), "https://models.example/catalog");
    assert_eq!(list.request.method(), Method::GET);
}

#[test]
fn shapes_cli_identity_from_the_conversation_body() {
    let settings = json!({});
    let user_body =
        Bytes::from_static(br#"{"model":"route","messages":[{"role":"user","content":"hi"}]}"#);
    let user = prepare(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        "gpt-5.4",
        &user_body,
        &secret(),
        &settings,
    );
    assert_eq!(
        user.request.headers()["authorization"],
        "Bearer copilot-short"
    );
    assert_eq!(user.request.headers()["x-initiator"], "user");
    assert_eq!(
        user.request.headers()["copilot-integration-id"],
        "copilot-developer-cli"
    );
    assert_eq!(user.profile, Some(&super::profile::CLIENT_PROFILE));
    let machine = user.request.headers()["x-client-machine-id"]
        .to_str()
        .unwrap();
    assert_eq!(machine.len(), 36);
    let shaped: Value = serde_json::from_slice(user.request.body()).unwrap();
    assert_eq!(shaped["model"], "gpt-5.4");

    let agent_body = Bytes::from_static(
        br#"{"model":"route","messages":[{"role":"assistant","content":"call"}]}"#,
    );
    let agent = prepare(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        "gpt-5.4",
        &agent_body,
        &secret(),
        &settings,
    );
    assert_eq!(agent.request.headers()["x-initiator"], "agent");
    assert_eq!(agent.request.headers()["x-client-machine-id"], machine);
    assert_ne!(
        agent.request.headers()["x-interaction-id"],
        user.request.headers()["x-interaction-id"]
    );
}

fn prepare(
    key: OperationKey,
    model: &str,
    body: &Bytes,
    secret: &Value,
    settings: &Value,
) -> gproxy_channel_api::PreparedRequest {
    CopilotCliChannel
        .prepare(PrepareCtx {
            key,
            stream: key.operation == Operation::StreamGenerateContent,
            method: &Method::PATCH,
            path: "/client/path",
            query: Some("ignored=yes"),
            headers: &HeaderMap::new(),
            body,
            upstream_model: model,
            provider_settings: settings,
            secret,
        })
        .unwrap()
}

fn secret() -> Value {
    json!({
        "github_token":"github-long",
        "copilot_token":"copilot-short",
        "copilot_expires_at_ms":9_000_000_000_000_i64
    })
}
