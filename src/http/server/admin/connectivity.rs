//! Fixed-destination outbound proxy connectivity check.
//!
//! The Console may select the proxy path, but never the destination: every
//! probe goes to Cloudflare's small public trace endpoint. This both avoids an
//! SSRF surface and reports the public egress IP/family seen by the Internet.

use std::net::IpAddr;
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::api::error::ApiError;
use crate::app::AppState;
use crate::http::client::ClientError;

const TRACE_URL: &str = "https://www.cloudflare.com/cdn-cgi/trace";
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TRACE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectivityScope {
    Global,
    Provider,
    Credential,
}

#[derive(Debug, Deserialize)]
pub struct ConnectivityTestRequest {
    pub scope: ConnectivityScope,
    /// Current unsaved form value. Blank/null means inherit according to scope.
    pub proxy_url: Option<String>,
    /// Required for credential scope so an empty credential proxy can inherit
    /// its provider proxy before the global proxy.
    pub provider_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ConnectivityTestResponse {
    pub ok: bool,
    pub ip: Option<String>,
    pub ip_version: Option<u8>,
    pub colo: Option<String>,
    pub location: Option<String>,
    pub latency_ms: u64,
    pub proxy_source: String,
    pub error_code: Option<String>,
    pub message: Option<String>,
}

impl ConnectivityTestResponse {
    fn failure(
        latency_ms: u64,
        proxy_source: String,
        error_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            ok: false,
            ip: None,
            ip_version: None,
            colo: None,
            location: None,
            latency_ms,
            proxy_source,
            error_code: Some(error_code.into()),
            message: Some(message.into()),
        }
    }
}

pub async fn test(
    State(state): State<AppState>,
    Json(body): Json<ConnectivityTestRequest>,
) -> Result<Json<ConnectivityTestResponse>, ApiError> {
    let current_proxy = body
        .proxy_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let (proxy, source) = resolve_proxy(&state, body.scope, body.provider_id, current_proxy)?;
    let client = match state.upstream_client_for_proxy(proxy.as_deref()) {
        Ok(client) => client,
        Err(error) => {
            return Ok(Json(ConnectivityTestResponse::failure(
                0,
                source,
                "invalid_proxy",
                client_error_message(&error),
            )));
        }
    };

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(TRACE_URL)
        .header(http::header::ACCEPT, "text/plain")
        .body(Bytes::new())
        .map_err(|error| ApiError::Internal(error.to_string()))?;

    let started = Instant::now();
    let probe = async {
        let (status, _headers, mut stream) = client.send_streaming(request).await?;
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
    let (status, response_body) = match tokio::time::timeout(PROBE_TIMEOUT, probe).await {
        Err(_) => {
            return Ok(Json(ConnectivityTestResponse::failure(
                elapsed_ms(started),
                source,
                "timeout",
                "connectivity test timed out",
            )));
        }
        Ok(Err(error)) => {
            let code = match error {
                ClientError::Config(_) => "invalid_proxy",
                ClientError::Transport(_) => "transport",
            };
            return Ok(Json(ConnectivityTestResponse::failure(
                elapsed_ms(started),
                source,
                code,
                client_error_message(&error),
            )));
        }
        Ok(Ok(response)) => response,
    };
    let latency_ms = elapsed_ms(started);

    if !status.is_success() {
        let code = if status == http::StatusCode::PROXY_AUTHENTICATION_REQUIRED {
            "proxy_auth"
        } else {
            "http_status"
        };
        return Ok(Json(ConnectivityTestResponse::failure(
            latency_ms,
            source,
            code,
            format!("probe returned HTTP {}", status.as_u16()),
        )));
    }

    let trace = parse_trace(&response_body);
    let Some(ip_text) = trace.ip else {
        return Ok(Json(ConnectivityTestResponse::failure(
            latency_ms,
            source,
            "invalid_response",
            "probe response did not contain a valid egress IP",
        )));
    };
    let ip: IpAddr = ip_text
        .parse()
        .map_err(|_| ApiError::Internal("validated Cloudflare trace IP failed to parse".into()))?;

    Ok(Json(ConnectivityTestResponse {
        ok: true,
        ip: Some(ip.to_string()),
        ip_version: Some(if ip.is_ipv4() { 4 } else { 6 }),
        colo: trace.colo,
        location: trace.location,
        latency_ms,
        proxy_source: source,
        error_code: None,
        message: None,
    }))
}

fn resolve_proxy(
    state: &AppState,
    scope: ConnectivityScope,
    provider_id: Option<i64>,
    current_proxy: Option<String>,
) -> Result<(Option<String>, String), ApiError> {
    if let Some(proxy) = current_proxy {
        let source = match scope {
            ConnectivityScope::Global => "global",
            ConnectivityScope::Provider => "provider",
            ConnectivityScope::Credential => "credential",
        };
        return Ok((Some(proxy), source.into()));
    }

    if matches!(scope, ConnectivityScope::Credential) {
        let provider_id = provider_id.ok_or_else(|| {
            ApiError::BadRequest("provider_id is required for credential scope".into())
        })?;
        let provider = state
            .cp()
            .providers_by_id
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("provider {provider_id} not found")))?;
        if let Some(proxy) = provider.proxy_url.clone() {
            return Ok((Some(proxy), "provider".into()));
        }
    }

    if !matches!(scope, ConnectivityScope::Global)
        && let Some(proxy) = state.cp().proxy.clone()
    {
        return Ok((Some(proxy), "global".into()));
    }
    if let Some(proxy) = state.config.upstream.proxy_url.clone() {
        return Ok((Some(proxy), "startup".into()));
    }
    Ok((None, "direct".into()))
}

fn client_error_message(error: &ClientError) -> String {
    match error {
        // Do not echo transport-library errors: they may include a proxy URL
        // containing embedded credentials. The stable error_code carries the
        // useful category for the Console.
        ClientError::Config(_) => "proxy configuration is invalid".into(),
        ClientError::Transport(_) => "connectivity probe failed".into(),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

#[derive(Default)]
struct TraceData {
    ip: Option<String>,
    colo: Option<String>,
    location: Option<String>,
}

fn parse_trace(body: &[u8]) -> TraceData {
    let mut trace = TraceData::default();
    for line in String::from_utf8_lossy(body).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key {
            "ip" if value.parse::<IpAddr>().is_ok() => trace.ip = Some(value.to_owned()),
            "colo" if !value.is_empty() => trace.colo = Some(value.to_owned()),
            "loc" if !value.is_empty() => trace.location = Some(value.to_owned()),
            _ => {}
        }
    }
    trace
}

#[cfg(test)]
mod tests {
    use super::parse_trace;

    #[test]
    fn parses_ipv4_trace() {
        let trace = parse_trace(b"fl=x\nip=203.0.113.8\ncolo=SIN\nloc=SG\n");
        assert_eq!(trace.ip.as_deref(), Some("203.0.113.8"));
        assert_eq!(trace.colo.as_deref(), Some("SIN"));
        assert_eq!(trace.location.as_deref(), Some("SG"));
    }

    #[test]
    fn rejects_invalid_trace_ip() {
        let trace = parse_trace(b"ip=not-an-ip\ncolo=LAX\n");
        assert!(trace.ip.is_none());
    }
}
