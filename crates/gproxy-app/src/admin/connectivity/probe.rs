use std::net::IpAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::StreamExt as _;
use gproxy_admin::dto::ConnectivityProbeDto;
use gproxy_core::{ProviderRef, UpstreamTransport};

pub(super) const TRACE_V4_URL: &str = "https://1.1.1.1/cdn-cgi/trace";
pub(super) const TRACE_V6_URL: &str = "https://[2606:4700:4700::1111]/cdn-cgi/trace";
const MAX_TRACE_BYTES: usize = 8 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct ProbeFailure {
    pub latency_ms: u64,
    pub code: &'static str,
    pub message: &'static str,
}

pub(super) async fn run(
    transport: &impl UpstreamTransport,
    provider: &ProviderRef,
    url: &'static str,
    ipv6: bool,
) -> Result<ConnectivityProbeDto, ProbeFailure> {
    let started = Instant::now();
    let run = async {
        let mut request = http::Request::get(url)
            .header(http::header::ACCEPT, "text/plain")
            .body(Bytes::new())
            .expect("trace URI is valid");
        gproxy_core::apply_provider_transport(&mut request, provider).map_err(|_| {
            failure(
                started,
                "invalid_fingerprint",
                "configured fingerprint is unusable",
            )
        })?;
        let response = transport
            .send(request)
            .await
            .map_err(|_| failure(started, "transport", "connectivity probe failed"))?;
        if !response.status().is_success() {
            return Err(failure(
                started,
                "http_status",
                "connectivity probe returned an error status",
            ));
        }
        let mut stream = response.into_body();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|_| failure(started, "transport", "connectivity probe failed"))?;
            let remaining = MAX_TRACE_BYTES.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if body.len() == MAX_TRACE_BYTES {
                break;
            }
        }
        parse_trace(&body, ipv6, elapsed(started))
    };
    tokio::time::timeout(PROBE_TIMEOUT, run)
        .await
        .unwrap_or_else(|_| Err(failure(started, "timeout", "connectivity probe timed out")))
}

fn parse_trace(
    body: &[u8],
    ipv6: bool,
    latency_ms: u64,
) -> Result<ConnectivityProbeDto, ProbeFailure> {
    let text = std::str::from_utf8(body).map_err(|_| ProbeFailure {
        latency_ms,
        code: "invalid_response",
        message: "connectivity probe returned an invalid response",
    })?;
    let value = |key: &str| {
        text.lines().find_map(|line| {
            line.split_once('=')
                .filter(|(name, _)| *name == key)
                .map(|(_, value)| value)
        })
    };
    let ip: IpAddr = value("ip")
        .and_then(|value| value.parse().ok())
        .filter(|ip: &IpAddr| ip.is_ipv6() == ipv6)
        .ok_or(ProbeFailure {
            latency_ms,
            code: "invalid_response",
            message: "connectivity probe did not report the expected egress IP",
        })?;
    Ok(ConnectivityProbeDto {
        ip: ip.to_string(),
        colo: value("colo").map(str::to_owned),
        location: value("loc").map(str::to_owned),
        latency_ms,
    })
}

fn elapsed(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

pub(super) fn latency(result: &Result<ConnectivityProbeDto, ProbeFailure>) -> u64 {
    result
        .as_ref()
        .map_or_else(|error| error.latency_ms, |value| value.latency_ms)
}

fn failure(started: Instant, code: &'static str, message: &'static str) -> ProbeFailure {
    ProbeFailure {
        latency_ms: elapsed(started),
        code,
        message,
    }
}
