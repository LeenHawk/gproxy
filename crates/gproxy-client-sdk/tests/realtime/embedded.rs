use super::support::*;
use gproxy_sdk::{ClientPool, GproxyBuilder, OutboundClient, SyncMode};
use protocol::capability::{CapabilityError, CapabilityFuture, UpstreamConnection};
use std::sync::{Arc, Mutex};

struct Upstream {
    script: Script,
    request: Mutex<Option<(String, String)>>,
}
impl OutboundClient for Upstream {
    fn send<'a>(
        &'a self,
        _: http::Request<protocol::HttpBody>,
    ) -> CapabilityFuture<
        'a,
        std::result::Result<protocol::WireResponse<protocol::HttpBody>, CapabilityError>,
    > {
        Box::pin(async { panic!("embedded Realtime must use connect") })
    }
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, std::result::Result<UpstreamConnection, CapabilityError>> {
        *self.request.lock().unwrap() = Some((
            request.uri().to_string(),
            request
                .headers()
                .get("authorization")
                .or_else(|| request.headers().get("x-goog-api-key"))
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
        ));
        Box::pin(async move {
            Ok(UpstreamConnection::Connected {
                handshake: protocol::WireResponse {
                    status: http::StatusCode::SWITCHING_PROTOCOLS,
                    headers: http::HeaderMap::new(),
                    body: (),
                },
                socket: self.script.socket(),
            })
        })
    }
}
#[tokio::test]
async fn embedded_routes_explicit_model_through_memory_custom_channel() {
    check(RealtimeDialect::Openai).await;
}
#[tokio::test]
async fn embedded_gemini_uses_explicit_routing_and_setup() {
    check(RealtimeDialect::Gemini).await;
}
async fn check(dialect: RealtimeDialect) {
    let is_openai = dialect == RealtimeDialect::Openai;
    let upstream = Arc::new(Upstream {
        script: Script::new(vec![if is_openai {
            ready()
        } else {
            json!({"setupComplete":{}})
        }]),
        request: Mutex::default(),
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
    let mut c = config(dialect);
    c.model.id = "local/native-model".into();
    let mut session = client.connect_realtime(&c).await.unwrap();
    assert!(matches!(
        session.events.next().await.unwrap().kind,
        RealtimeEventKind::Ready
    ));
    session.sender.send_text("hello").await.unwrap();
    let request = upstream.request.lock().unwrap();
    let (uri, auth) = request.as_ref().unwrap();
    if is_openai {
        assert!(uri.starts_with("https://upstream.test/v1/realtime?model="));
        assert_eq!(auth, "Bearer upstream-key");
        assert_eq!(upstream.script.values()[0]["type"], "session.update");
        assert_eq!(
            upstream.script.values()[1]["item"]["content"][0]["text"],
            "hello"
        );
    } else {
        assert!(uri.ends_with(
            "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
        ));
        assert_eq!(auth, "upstream-key");
        assert_eq!(
            upstream.script.values()[0]["setup"]["model"],
            "models/local/native-model"
        );
        assert_eq!(
            upstream.script.values()[1]["clientContent"]["turns"][0]["parts"][0]["text"],
            "hello"
        );
    }
}
