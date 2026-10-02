#![cfg(feature = "glm")]

mod support;
use gproxy_channel::channel::{
    CredentialContext, NoState, OperationContext, QuotaQuery, QuotaValue,
};
use gproxy_channel::channels::glm::Glm;
use gproxy_channel::{BaseChannel, OutboundClient};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::{Dialect, HttpBody, WireResponse};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{OneShot, credential, provider, request};

struct Catalogue {
    seen: Mutex<Vec<(String, HeaderMap)>>,
}
impl OutboundClient for Catalogue {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let mut seen = self.seen.lock().unwrap();
            seen.push((request.uri().to_string(), request.headers().clone()));
            let body = if seen.len() == 1 {
                json!({"code":0,"data":{"configs":{"builtin_provider_config_json":"https://cdn.example/latest.json"}}})
            } else {
                json!({"schemaVersion":1,"revision":99,"config":{"providerConfigRules":{"templateRules":[
                    {"templateId":"bigmodel-api","config":{"builtinModelIds":["new-coding-model"]}},
                    {"templateId":"bigmodel-standard-api","config":{"builtinModelIds":["new-platform-model"]}}
                ]}}})
            };
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(body.to_string().into()),
            })
        })
    }
}

#[tokio::test]
async fn live_catalogue_templates_separate_products_without_sending_credentials_to_the_cdn() {
    for (channel, expected) in [
        (Glm::API, "new-platform-model"),
        (Glm::CODING, "new-coding-model"),
    ] {
        let config = json!({});
        let secret = json!({"api_key":"private-account-key"});
        let client = Arc::new(Catalogue {
            seen: Mutex::new(vec![]),
        });
        let response = channel
            .list_models(OperationContext {
                provider: provider(channel.id(), &config, None),
                credential: credential("api_key", &secret, &Value::Null),
                dialect: Dialect::OpenAi,
                request: request("/v1/models", None),
                client: client.clone(),
                state: Arc::new(NoState::default()),
                instance_id: "test".into(),
                endpoint_override: None,
            })
            .await
            .unwrap();
        let HttpBody::Bytes(body) = response.body else {
            panic!("JSON")
        };
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["data"][0]["id"], expected);
        assert_eq!(body["data"][0]["catalog_revision"], 99);
        let seen = client.seen.lock().unwrap();
        assert!(
            seen[0]
                .0
                .starts_with("https://zcode.z.ai/api/v1/client/configs?")
        );
        assert_eq!(seen[1].0, "https://cdn.example/latest.json");
        assert!(seen.iter().all(|(_, headers)| headers.is_empty()));
    }
    assert!(Glm::API.quota_query().is_none());
}

#[tokio::test]
async fn coding_quota_keeps_independent_windows_and_uses_raw_key_authentication() {
    let config = json!({});
    let secret = json!({"api_key":"coding-key"});
    let client = OneShot::new(StatusCode::OK, json!({"code":200,"success":true,"data":{"limits":[
        {"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":40,"nextResetTime":1791000000000i64},
        {"type":"TOKENS_LIMIT","unit":6,"number":1,"percentage":80,"nextResetTime":1791500000000i64}
    ]}}).to_string());
    let snapshot = Glm::CODING
        .query(CredentialContext {
            provider: provider("glmcode", &config, None),
            credential: credential("api_key", &secret, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(
        client.call(0).0,
        "https://open.bigmodel.cn/api/monitor/usage/quota/limit"
    );
    assert_eq!(client.call(0).1["authorization"], "coding-key");
    assert_ne!(snapshot.entries[0].id, snapshot.entries[1].id);
    let QuotaValue::Window(window) = &snapshot.entries[1].value else {
        panic!("window")
    };
    assert_eq!(window.used_percent, Some(80.into()));
    assert_eq!(window.period_end_ms, Some(1791500000000));
    assert_eq!(window.remaining, None, "unreported counts are not invented");
}
