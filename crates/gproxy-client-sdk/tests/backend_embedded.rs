#![cfg(all(feature = "embedded", feature = "openai", not(target_arch = "wasm32")))]
mod support;
use gproxy_client_sdk::*;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use gproxy_sdk::{ClientPool, GproxyBuilder, OutboundClient, SyncMode};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct Upstream {
    request: Mutex<Option<Value>>,
}
impl OutboundClient for Upstream {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, std::result::Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(
                request.uri().to_string(),
                "https://upstream.test/v1/responses"
            );
            let bytes = gproxy_protocol::codec::read_http_body(request.into_body(), DEFAULT_LIMITS)
                .await
                .unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["model"], "native-model");
            *self.request.lock().unwrap() = Some(body.clone());
            let streaming = body["stream"] == true;
            let native = support::openai();
            let data = if streaming {
                support::openai_events(&native)
                    .into_iter()
                    .map(|v| format!("data: {v}\n\n"))
                    .collect::<String>()
            } else {
                native.to_string()
            };
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(if streaming {
                    "text/event-stream"
                } else {
                    "application/json"
                }),
            );
            Ok(WireResponse {
                status: http::StatusCode::OK,
                headers,
                body: HttpBody::Bytes(Bytes::from(data)),
            })
        })
    }
}

#[tokio::test]
async fn embedded_uses_call_scope_native_generation_and_the_local_catalog() {
    let upstream = Arc::new(Upstream {
        request: Mutex::new(None),
    });
    let engine = Arc::new(
        GproxyBuilder::sqlite_memory()
            .await
            .unwrap()
            .plaintext_secrets()
            .without_default_channels()
            .channel(Arc::new(gproxy_sdk::channels::custom::Custom))
            .client_pool(ClientPool::with_client(upstream.clone()))
            .sync_mode(SyncMode::Manual)
            .build()
            .await
            .unwrap(),
    );
    let provider = engine
        .manage()
        .providers()
        .create(gproxy_sdk::dto::ProviderWrite {
            name: "local".into(),
            channel: "custom".into(),
            base_url: Some("https://upstream.test".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    engine
        .manage()
        .credentials()
        .create(gproxy_sdk::dto::CredentialWrite {
            provider_id: provider.id.clone(),
            auth_kind: "api_key".into(),
            secret: json!({"api_key":"upstream-key"}),
            ..Default::default()
        })
        .await
        .unwrap();
    engine
        .manage()
        .provider_models()
        .create(gproxy_sdk::dto::ProviderModelWrite {
            provider_id: provider.id,
            upstream_name: "native-model".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let client = Client::new(backend::Embedded::new(engine, "app:test"));
    let request = GenerateRequest::new(
        Model {
            id: "local/native-model".into(),
            dialect: Dialect::Openai,
        },
        vec![Message {
            role: Role::User,
            content: vec![Block::text("hello")],
        }],
    );
    let future = client.generate(&request);
    fn is_send<T: Send>(_: &T) {}
    is_send(&future);
    let result = future.await.unwrap();
    assert_eq!(result.usage.input_tokens, Some(12));
    assert_eq!(
        client
            .stream(&request)
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap(),
        result
    );
    #[cfg(feature = "models")]
    {
        for dialect in [Dialect::Openai, Dialect::Claude, Dialect::Gemini] {
            let models = client.list_models(dialect).await.unwrap();
            let model = Model {
                id: "local/native-model".into(),
                dialect,
            };
            assert!(models.iter().any(|m| m.model == model), "{dialect:?}");
            assert_eq!(client.get_model(&model).await.unwrap().model, model);
        }
        let missing = client
            .get_model(&Model {
                id: "local/absent".into(),
                dialect: Dialect::Gemini,
            })
            .await
            .unwrap_err();
        assert_eq!(missing.status, Some(404));
    }
}
