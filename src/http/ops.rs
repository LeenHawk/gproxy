//! Target-independent response core for operational endpoints.

use std::fmt::Write as _;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, StatusCode, header};

use crate::store::persistence::metrics::{LATENCY_BUCKETS_MS, MetricsAggregate};

pub(crate) struct OpsResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub(crate) fn healthz() -> OpsResponse {
    response(
        StatusCode::OK,
        "application/json",
        Bytes::from_static(br#"{"status":"ok"}"#),
    )
}

pub(crate) fn version() -> OpsResponse {
    const BODY: &str = concat!(r#"{"version":""#, env!("CARGO_PKG_VERSION"), r#""}"#);
    response(
        StatusCode::OK,
        "application/json",
        Bytes::from_static(BODY.as_bytes()),
    )
}

pub(crate) fn metrics(aggregate: Option<&MetricsAggregate>) -> OpsResponse {
    match aggregate {
        Some(aggregate) => response(
            StatusCode::OK,
            "text/plain; version=0.0.4",
            Bytes::from(render_metrics(aggregate)),
        ),
        None => response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "text/plain; charset=utf-8",
            Bytes::from_static(b"metrics unavailable"),
        ),
    }
}

fn response(status: StatusCode, content_type: &'static str, body: Bytes) -> OpsResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    OpsResponse {
        status,
        headers,
        body,
    }
}

/// Build the Prometheus exposition body for the current metrics snapshot.
fn render_metrics(m: &MetricsAggregate) -> String {
    let mut s = String::with_capacity(1024);

    metric(
        &mut s,
        "gproxy_requests_total",
        "counter",
        "Total settled requests.",
    );
    let _ = writeln!(s, "gproxy_requests_total {}", m.requests_total);

    metric(
        &mut s,
        "gproxy_tokens_total",
        "counter",
        "Total tokens by direction.",
    );
    let _ = writeln!(
        s,
        "gproxy_tokens_total{{direction=\"input\"}} {}",
        m.input_tokens_total
    );
    let _ = writeln!(
        s,
        "gproxy_tokens_total{{direction=\"output\"}} {}",
        m.output_tokens_total
    );

    metric(
        &mut s,
        "gproxy_upstream_latency_ms",
        "histogram",
        "Upstream time-to-first-response latency (ms).",
    );
    for (i, le) in LATENCY_BUCKETS_MS.iter().enumerate() {
        let value = m.latency_buckets.get(i).copied().unwrap_or(0);
        let _ = writeln!(
            s,
            "gproxy_upstream_latency_ms_bucket{{le=\"{le}\"}} {value}"
        );
    }
    let _ = writeln!(
        s,
        "gproxy_upstream_latency_ms_bucket{{le=\"+Inf\"}} {}",
        m.latency_count
    );
    let _ = writeln!(s, "gproxy_upstream_latency_ms_sum {}", m.latency_sum_ms);
    let _ = writeln!(s, "gproxy_upstream_latency_ms_count {}", m.latency_count);

    if !m.credential_health.is_empty() {
        metric(
            &mut s,
            "gproxy_credential_health",
            "gauge",
            "Credential count by health kind.",
        );
        for (kind, count) in &m.credential_health {
            let _ = writeln!(
                s,
                "gproxy_credential_health{{health_kind=\"{}\"}} {count}",
                escape(kind)
            );
        }
    }

    if !m.quota.is_empty() {
        metric(
            &mut s,
            "gproxy_quota_total",
            "gauge",
            "Quota total by scope.",
        );
        for quota in &m.quota {
            let _ = writeln!(
                s,
                "gproxy_quota_total{{scope=\"{}\",scope_id=\"{}\"}} {}",
                quota.scope, quota.scope_id, quota.total
            );
        }
        metric(&mut s, "gproxy_quota_used", "gauge", "Quota used by scope.");
        for quota in &m.quota {
            let _ = writeln!(
                s,
                "gproxy_quota_used{{scope=\"{}\",scope_id=\"{}\"}} {}",
                quota.scope, quota.scope_id, quota.used
            );
        }
    }

    s
}

fn metric(s: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(s, "# HELP {name} {help}");
    let _ = writeln!(s, "# TYPE {name} {kind}");
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::metrics::QuotaUsage;

    #[test]
    fn builds_operational_responses() {
        let health = healthz();
        assert_eq!(health.status, StatusCode::OK);
        assert_eq!(health.headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(health.body, br#"{"status":"ok"}"#[..]);

        let version = version();
        assert_eq!(version.headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            version.body,
            format!(r#"{{"version":"{}"}}"#, env!("CARGO_PKG_VERSION"))
        );

        let unavailable = metrics(None);
        assert_eq!(unavailable.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            unavailable.headers[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        assert_eq!(unavailable.body, b"metrics unavailable"[..]);
    }

    #[test]
    fn renders_metrics_response() {
        let aggregate = MetricsAggregate {
            requests_total: 42,
            input_tokens_total: 1000,
            output_tokens_total: 500,
            latency_buckets: vec![1, 2, 3, 4, 5, 6, 7, 8],
            latency_sum_ms: 12345,
            latency_count: 8,
            credential_health: vec![("healthy".into(), 3), ("cooldown".into(), 1)],
            quota: vec![QuotaUsage {
                scope: "user".into(),
                scope_id: 9,
                total: "100".parse().unwrap(),
                used: "12.5".parse().unwrap(),
            }],
        };

        let response = metrics(Some(&aggregate));
        let body = std::str::from_utf8(&response.body).unwrap();
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.headers[header::CONTENT_TYPE],
            "text/plain; version=0.0.4"
        );
        assert!(body.contains("gproxy_requests_total 42"));
        assert!(body.contains("gproxy_upstream_latency_ms_bucket{le=\"+Inf\"} 8"));
        assert!(body.contains("gproxy_credential_health{health_kind=\"healthy\"} 3"));
        assert!(body.contains("gproxy_quota_used{scope=\"user\",scope_id=\"9\"} 12.5"));
    }
}
