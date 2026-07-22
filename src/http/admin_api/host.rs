//! Host-level admin operations that are available only on native builds.

use bytes::Bytes;
use http::Method;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Request, Resp, json_body, segments};

#[derive(Debug, Deserialize)]
struct SetAutoStart {
    enabled: bool,
}

pub(super) async fn dispatch(
    state: &AppState,
    request: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let result = match (&request.method, segments(request).as_slice()) {
        (&Method::GET, ["admin", "autostart"]) => autostart_status(state, request).await,
        (&Method::PUT, ["admin", "autostart"]) => autostart_set(state, request, body).await,
        (&Method::POST, ["admin", "connectivity", "test"]) => {
            connectivity_test(state, request, body).await
        }
        _ => return None,
    };
    Some(result)
}

async fn autostart_status(state: &AppState, request: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        Resp::json(200, &state.autostart.status())
    }
    #[cfg(target_arch = "wasm32")]
    Err(ApiError::NotImplemented(
        "automatic startup is unavailable on edge".into(),
    ))
}

async fn autostart_set(
    state: &AppState,
    request: &Request,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    let input: SetAutoStart = json_body(body)?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let current = state.autostart.status();
        if !current.supported && !current.enabled {
            return Err(ApiError::BadRequest(
                current
                    .detail
                    .unwrap_or_else(|| "automatic startup is unavailable".into()),
            ));
        }
        if !current.supported && input.enabled {
            return Err(ApiError::BadRequest(
                current
                    .detail
                    .unwrap_or_else(|| "automatic startup cannot be enabled".into()),
            ));
        }
        let status = state
            .autostart
            .set_enabled(input.enabled)
            .map_err(|error| ApiError::Internal(error.to_string()))?;
        Resp::json(200, &status)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = input.enabled;
        Err(ApiError::NotImplemented(
            "automatic startup is unavailable on edge".into(),
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod connectivity {
    use std::net::IpAddr;
    use std::time::{Duration, Instant};

    use futures_util::StreamExt as _;
    use serde::{Deserialize, Serialize};

    use crate::http::client::ClientError;

    use super::*;

    const TRACE_V4_URL: &str = "https://1.1.1.1/cdn-cgi/trace";
    const TRACE_V6_URL: &str = "https://[2606:4700:4700::1111]/cdn-cgi/trace";
    const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
    const MAX_TRACE_BYTES: usize = 8 * 1024;

    #[derive(Debug, Clone, Copy, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Scope {
        Global,
        Provider,
        Credential,
    }

    #[derive(Debug, Deserialize)]
    pub(super) struct TestRequest {
        scope: Scope,
        proxy_url: Option<String>,
        provider_id: Option<i64>,
    }

    #[derive(Debug, Serialize)]
    struct TestResponse {
        ok: bool,
        ipv4: Option<ProbeResponse>,
        ipv6: Option<ProbeResponse>,
        latency_ms: u64,
        proxy_source: String,
        error_code: Option<String>,
        message: Option<String>,
    }

    #[derive(Debug, Serialize)]
    struct ProbeResponse {
        ip: String,
        colo: Option<String>,
        location: Option<String>,
        latency_ms: u64,
    }

    struct ProbeFailure {
        latency_ms: u64,
        code: &'static str,
        message: String,
    }

    impl TestResponse {
        fn failure(latency_ms: u64, source: String, code: &str, message: String) -> Self {
            Self {
                ok: false,
                ipv4: None,
                ipv6: None,
                latency_ms,
                proxy_source: source,
                error_code: Some(code.into()),
                message: Some(message),
            }
        }
    }

    pub(super) async fn run(state: &AppState, input: TestRequest) -> Result<Resp, ApiError> {
        let current = input
            .proxy_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let (proxy, source) = resolve_proxy(state, input.scope, input.provider_id, current)?;
        let client = match state.upstream_client_for_proxy(proxy.as_deref()) {
            Ok(client) => client,
            Err(error) => {
                return response(TestResponse::failure(
                    0,
                    source,
                    "invalid_proxy",
                    client_error(&error),
                ));
            }
        };
        let (ipv4_result, ipv6_result) = tokio::join!(
            probe(client.as_ref(), TRACE_V4_URL, false),
            probe(client.as_ref(), TRACE_V6_URL, true),
        );
        let latency = ipv4_result
            .as_ref()
            .map_or_else(|error| error.latency_ms, |probe| probe.latency_ms)
            .max(
                ipv6_result
                    .as_ref()
                    .map_or_else(|error| error.latency_ms, |probe| probe.latency_ms),
            );
        let (ipv4, ipv4_error) = match ipv4_result {
            Ok(probe) => (Some(probe), None),
            Err(error) => (None, Some(error)),
        };
        let ipv6 = ipv6_result.ok();
        if ipv4.is_some() || ipv6.is_some() {
            return response(TestResponse {
                ok: true,
                ipv4,
                ipv6,
                latency_ms: latency,
                proxy_source: source,
                error_code: None,
                message: None,
            });
        }

        let error = ipv4_error.expect("an absent IPv4 result has an error");
        response(TestResponse::failure(
            latency,
            source,
            error.code,
            error.message,
        ))
    }

    async fn probe(
        client: &dyn crate::http::client::UpstreamClient,
        url: &'static str,
        expect_ipv6: bool,
    ) -> Result<ProbeResponse, ProbeFailure> {
        let request = http::Request::builder()
            .method(Method::GET)
            .uri(url)
            .header(http::header::ACCEPT, "text/plain")
            .body(Bytes::new())
            .expect("Cloudflare trace URL is valid");
        let started = Instant::now();
        let probe = async {
            let (status, _, mut stream) = client.send_streaming(request).await?;
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                let remaining = MAX_TRACE_BYTES.saturating_sub(body.len());
                body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                if body.len() == MAX_TRACE_BYTES {
                    break;
                }
            }
            Ok::<_, ClientError>((status, body))
        };
        let (status, body) = match tokio::time::timeout(PROBE_TIMEOUT, probe).await {
            Err(_) => {
                return Err(ProbeFailure {
                    latency_ms: elapsed(started),
                    code: "timeout",
                    message: "connectivity test timed out".into(),
                });
            }
            Ok(Err(error)) => {
                let code = if matches!(error, ClientError::Config(_)) {
                    "invalid_proxy"
                } else {
                    "transport"
                };
                return Err(ProbeFailure {
                    latency_ms: elapsed(started),
                    code,
                    message: client_error(&error),
                });
            }
            Ok(Ok(value)) => value,
        };
        let latency = elapsed(started);
        if !status.is_success() {
            let code = if status == http::StatusCode::PROXY_AUTHENTICATION_REQUIRED {
                "proxy_auth"
            } else {
                "http_status"
            };
            return Err(ProbeFailure {
                latency_ms: latency,
                code,
                message: format!("probe returned HTTP {}", status.as_u16()),
            });
        }
        let trace = parse_trace(&body);
        let Some(ip_text) = trace.ip else {
            return Err(ProbeFailure {
                latency_ms: latency,
                code: "invalid_response",
                message: "probe response did not contain a valid egress IP".into(),
            });
        };
        let ip: IpAddr = ip_text
            .parse()
            .expect("parse_trace only returns validated IP addresses");
        if ip.is_ipv6() != expect_ipv6 {
            return Err(ProbeFailure {
                latency_ms: latency,
                code: "invalid_response",
                message: "probe returned the wrong IP address family".into(),
            });
        }
        Ok(ProbeResponse {
            ip: ip.to_string(),
            colo: trace.colo,
            location: trace.location,
            latency_ms: latency,
        })
    }

    fn resolve_proxy(
        state: &AppState,
        scope: Scope,
        provider_id: Option<i64>,
        current: Option<String>,
    ) -> Result<(Option<String>, String), ApiError> {
        if let Some(proxy) = current {
            let source = match scope {
                Scope::Global => "global",
                Scope::Provider => "provider",
                Scope::Credential => "credential",
            };
            return Ok((Some(proxy), source.into()));
        }
        if matches!(scope, Scope::Credential) {
            let id = provider_id.ok_or_else(|| {
                ApiError::BadRequest("provider_id is required for credential scope".into())
            })?;
            let provider = state
                .cp()
                .providers_by_id
                .get(&id)
                .cloned()
                .ok_or_else(|| ApiError::NotFound(format!("provider {id} not found")))?;
            if let Some(proxy) = provider.proxy_url.clone() {
                return Ok((Some(proxy), "provider".into()));
            }
        }
        if !matches!(scope, Scope::Global)
            && let Some(proxy) = state.cp().proxy.clone()
        {
            return Ok((Some(proxy), "global".into()));
        }
        if let Some(proxy) = state.config.upstream.proxy_url.clone() {
            return Ok((Some(proxy), "startup".into()));
        }
        Ok((None, "direct".into()))
    }

    fn client_error(error: &ClientError) -> String {
        match error {
            ClientError::Config(_) => "proxy configuration is invalid",
            ClientError::Transport(_) => "connectivity probe failed",
        }
        .into()
    }

    fn elapsed(started: Instant) -> u64 {
        started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    #[derive(Default)]
    struct Trace {
        ip: Option<String>,
        colo: Option<String>,
        location: Option<String>,
    }

    fn parse_trace(body: &[u8]) -> Trace {
        let mut trace = Trace::default();
        for line in String::from_utf8_lossy(body).lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key {
                "ip" if value.parse::<IpAddr>().is_ok() => trace.ip = Some(value.into()),
                "colo" if !value.is_empty() => trace.colo = Some(value.into()),
                "loc" if !value.is_empty() => trace.location = Some(value.into()),
                _ => {}
            }
        }
        trace
    }

    fn response(value: TestResponse) -> Result<Resp, ApiError> {
        Resp::json(200, &value)
    }
}

async fn connectivity_test(
    state: &AppState,
    request: &Request,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        connectivity::run(state, json_body(body)?).await
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = body;
        Err(ApiError::NotImplemented(
            "connectivity testing is unavailable on edge".into(),
        ))
    }
}
