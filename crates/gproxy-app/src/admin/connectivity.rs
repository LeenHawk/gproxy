use gproxy_admin::dto::*;
use gproxy_core::UpstreamTransport;

use gproxy_admin::AdminError;

use crate::AppHandle;

mod probe;
pub(super) mod target;

pub(super) async fn run(
    app: &AppHandle,
    request: &ConnectivityTestRequest,
    transport: &impl UpstreamTransport,
) -> Result<ConnectivityTestResponse, AdminError> {
    let (provider, proxy_source) = target::resolve(app, request)?;
    let (ipv4, ipv6) = tokio::join!(
        probe::run(transport, &provider, probe::TRACE_V4_URL, false),
        probe::run(transport, &provider, probe::TRACE_V6_URL, true),
    );
    let latency_ms = probe::latency(&ipv4).max(probe::latency(&ipv6));
    let (ipv4, ipv4_error) = match ipv4 {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    let ipv6 = ipv6.ok();
    if ipv4.is_some() || ipv6.is_some() {
        return Ok(ConnectivityTestResponse {
            ok: true,
            ipv4,
            ipv6,
            latency_ms,
            proxy_source,
            error_code: None,
            message: None,
        });
    }
    let failure = ipv4_error.expect("an absent IPv4 result has an error");
    Ok(ConnectivityTestResponse {
        ok: false,
        ipv4: None,
        ipv6: None,
        latency_ms,
        proxy_source,
        error_code: Some(failure.code.into()),
        message: Some(failure.message.into()),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bytes::Bytes;
    use gproxy_channel_api::{BoxFuture, ByteStream, TransportError, WsDuplex};
    use gproxy_core::{UpstreamProxy, UpstreamTransport};
    use serde_json::json;

    use super::*;

    struct RecordingTransport(Mutex<Vec<String>>);

    impl UpstreamTransport for RecordingTransport {
        fn send<'a>(
            &'a self,
            request: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<http::Response<ByteStream>, TransportError>> {
            self.0.lock().unwrap().push(
                request
                    .extensions()
                    .get::<UpstreamProxy>()
                    .expect("probe proxy")
                    .0
                    .clone(),
            );
            let ipv6 = request.uri().host().is_some_and(|host| host.contains(':'));
            Box::pin(async move {
                let ip = if ipv6 { "2001:db8::1" } else { "192.0.2.1" };
                let body = Bytes::from(format!("ip={ip}\ncolo=TEST\nloc=ZZ\n"));
                let stream: ByteStream = Box::pin(futures_util::stream::once(async { Ok(body) }));
                Ok(http::Response::new(stream))
            })
        }

        fn open_websocket<'a>(
            &'a self,
            _: http::Request<Bytes>,
        ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
            Box::pin(async { Err(TransportError::Connect("unused".into())) })
        }
    }

    #[tokio::test]
    async fn credential_and_standalone_probes_use_their_configured_proxies() {
        let directory = tempfile::tempdir().unwrap();
        let app = crate::App::start(crate::Config::sqlite(
            "127.0.0.1:0".parse().unwrap(),
            directory.path().to_path_buf(),
            crate::MasterKeyConfig::new(Some([91; 32])),
        ))
        .await
        .unwrap();
        let provider = app
            .inner
            .host
            .services
            .store
            .insert_provider(&gproxy_store::records::ProviderInput {
                name: "probe-provider".into(),
                label: None,
                channel: "openai".into(),
                settings: json!({}),
                credential_strategy: "round_robin".into(),
                proxy_url: Some("http://provider-proxy.invalid".into()),
                tls_fingerprint: None,
                enabled: true,
            })
            .await
            .unwrap();
        let envelope =
            gproxy_admin::State::seal_credential(&app, &json!({"api_key": "unused"})).unwrap();
        let credential = app
            .inner
            .host
            .services
            .store
            .insert_credential(&gproxy_store::records::CredentialInput {
                provider_id: provider,
                label: None,
                kind: "api_key".into(),
                envelope,
                enabled: true,
                weight: 100,
                rpm_limit: None,
                tpm_limit: None,
                proxy_url: Some("http://credential-proxy.invalid".into()),
                tls_fingerprint: None,
            })
            .await
            .unwrap();
        app.reload().await.unwrap();
        let transport = RecordingTransport(Mutex::new(Vec::new()));
        let result = run(
            &app,
            &ConnectivityTestRequest {
                scope: ConnectivityScopeDto::Credential,
                provider_id: None,
                credential_id: Some(credential),
                proxy_url: None,
            },
            &transport,
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.ipv4.unwrap().ip, "192.0.2.1");
        assert_eq!(result.proxy_source, ConnectivityProxySourceDto::Credential);
        assert_eq!(
            transport.0.lock().unwrap().as_slice(),
            [
                "http://credential-proxy.invalid",
                "http://credential-proxy.invalid",
            ]
        );
        let standalone = run(
            &app,
            &ConnectivityTestRequest {
                scope: ConnectivityScopeDto::Proxy,
                provider_id: None,
                credential_id: None,
                proxy_url: Some("http://standalone-proxy.invalid".into()),
            },
            &transport,
        )
        .await
        .unwrap();
        assert!(standalone.ok);
        assert_eq!(standalone.proxy_source, ConnectivityProxySourceDto::Proxy);
        assert_eq!(
            &transport.0.lock().unwrap()[2..],
            [
                "http://standalone-proxy.invalid",
                "http://standalone-proxy.invalid",
            ]
        );
    }
}
