//! Counting a portable generation request through each dialect's count endpoint.
mod support;

use gproxy_client_sdk::*;
use serde_json::{Value, json};
use support::script::BackendScript;

#[cfg(all(
    feature = "count-tokens",
    feature = "claude",
    feature = "openai",
    feature = "gemini",
    feature = "chat"
))]
#[test]
fn counting_a_generation_request_uses_its_dialects_count_endpoint() {
    let messages: Vec<Message> = serde_json::from_value(json!([
        {"role": "system", "content": [{"type": "text", "text": "Be brief."}]},
        {"role": "user", "content": [{"type": "text", "text": "hello"}]}
    ]))
    .unwrap();
    let request = |dialect| {
        let mut request = GenerateRequest::new(
            Model {
                id: "model".into(),
                dialect,
            },
            messages.clone(),
        );
        request.settings.temperature = Some(0.5);
        request
    };
    futures_executor::block_on(async {
        let client = Client::new(BackendScript::new(vec![
            json!({"input_tokens": 12}),
            json!({"object": "response.input_tokens", "input_tokens": 13}),
            json!({"totalTokens": 14}),
        ]));

        // Claude: no output budget needed, and none is sent.
        let count = client.count(&request(Dialect::Claude)).await.unwrap();
        assert_eq!(count.input_tokens, Some(12));
        let sent = client.backend.take();
        assert_eq!(sent.key.operation, gproxy_protocol::Operation::CountTokens);
        assert_eq!(sent.path, "/v1/messages/count_tokens");
        let body: Value = serde_json::from_slice(&sent.body).unwrap();
        assert_eq!(body["model"], "model");
        assert_eq!(body["messages"][0]["role"], "user");
        assert!(body["system"].to_string().contains("Be brief."));
        for generation_only in ["max_tokens", "stream", "temperature"] {
            assert!(body.get(generation_only).is_none(), "{generation_only}");
        }

        let count = client.count(&request(Dialect::Openai)).await.unwrap();
        assert_eq!(count.input_tokens, Some(13));
        let sent = client.backend.take();
        assert_eq!(sent.path, "/v1/responses/input_tokens");
        let body: Value = serde_json::from_slice(&sent.body).unwrap();
        assert_eq!(body["model"], "model");
        assert!(body["input"].to_string().contains("hello"));
        assert!(body.get("temperature").is_none() && body.get("stream").is_none());

        // Gemini: the generation request embedded, model as a resource.
        let count = client.count(&request(Dialect::Gemini)).await.unwrap();
        assert_eq!(count.input_tokens, Some(14));
        let sent = client.backend.take();
        assert_eq!(sent.path, "/v1beta/models/model:countTokens");
        let body: Value = serde_json::from_slice(&sent.body).unwrap();
        let embedded = &body["generateContentRequest"];
        assert_eq!(embedded["model"], "models/model");
        assert!(embedded["contents"].to_string().contains("hello"));
        assert!(
            embedded["systemInstruction"]
                .to_string()
                .contains("Be brief.")
        );

        // Chat Completions has no count endpoint: nothing is sent.
        let error = client
            .count(&request(Dialect::OpenaiChat))
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported);
        assert!(client.backend.requests.lock().unwrap().is_empty());
    });
}
