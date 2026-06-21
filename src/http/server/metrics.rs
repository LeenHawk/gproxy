//! §15.3 `GET /metrics` — Prometheus text exposition, rendered from a
//! persistence-derived [`MetricsAggregate`] (never in-memory counters), so the
//! numbers are a cross-instance-consistent global aggregate and the same
//! renderer serves native and the wasm edge.

use std::fmt::Write as _;

#[cfg(not(target_arch = "wasm32"))]
use crate::app::AppState;
use crate::store::persistence::metrics::{LATENCY_BUCKETS_MS, MetricsAggregate};

/// Build the Prometheus exposition body for the current metrics snapshot.
/// Pure (no I/O) so it is trivially testable and target-agnostic.
pub fn render(m: &MetricsAggregate) -> String {
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

    // Upstream latency histogram. Buckets are cumulative `le`; render the
    // implicit `+Inf` bucket as the total count (Prometheus convention).
    metric(
        &mut s,
        "gproxy_upstream_latency_ms",
        "histogram",
        "Upstream time-to-first-response latency (ms).",
    );
    for (i, le) in LATENCY_BUCKETS_MS.iter().enumerate() {
        let v = m.latency_buckets.get(i).copied().unwrap_or(0);
        let _ = writeln!(s, "gproxy_upstream_latency_ms_bucket{{le=\"{le}\"}} {v}");
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
        for (kind, n) in &m.credential_health {
            let _ = writeln!(
                s,
                "gproxy_credential_health{{health_kind=\"{}\"}} {n}",
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
        for q in &m.quota {
            let _ = writeln!(
                s,
                "gproxy_quota_total{{scope=\"{}\",scope_id=\"{}\"}} {}",
                q.scope, q.scope_id, q.total
            );
        }
        metric(&mut s, "gproxy_quota_used", "gauge", "Quota used by scope.");
        for q in &m.quota {
            let _ = writeln!(
                s,
                "gproxy_quota_used{{scope=\"{}\",scope_id=\"{}\"}} {}",
                q.scope, q.scope_id, q.used
            );
        }
    }

    s
}

/// Emit the `# HELP` / `# TYPE` preamble for one metric family.
fn metric(s: &mut String, name: &str, kind: &str, help: &str) {
    let _ = writeln!(s, "# HELP {name} {help}");
    let _ = writeln!(s, "# TYPE {name} {kind}");
}

/// Escape a Prometheus label value (backslash, double-quote, newline).
fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// `GET /metrics` axum handler (native). The edge path renders the same body
/// via [`render`] from its fetch dispatcher. Sits behind the same
/// admin-session gate as `/admin/*` (the `require_admin` middleware owns the
/// auth).
#[cfg(not(target_arch = "wasm32"))]
pub async fn metrics(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    use axum::http::{StatusCode, header::CONTENT_TYPE};
    use axum::response::IntoResponse;

    match state.persistence.metrics_aggregate().await {
        Ok(agg) => ([(CONTENT_TYPE, "text/plain; version=0.0.4")], render(&agg)).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "metrics aggregate failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "metrics unavailable").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::metrics::QuotaUsage;

    #[test]
    fn renders_prometheus_text() {
        let agg = MetricsAggregate {
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
        let out = render(&agg);
        assert!(out.contains("gproxy_requests_total 42"));
        assert!(out.contains("gproxy_tokens_total{direction=\"input\"} 1000"));
        assert!(out.contains("gproxy_upstream_latency_ms_bucket{le=\"50\"} 1"));
        assert!(out.contains("gproxy_upstream_latency_ms_bucket{le=\"+Inf\"} 8"));
        assert!(out.contains("gproxy_upstream_latency_ms_count 8"));
        assert!(out.contains("gproxy_credential_health{health_kind=\"healthy\"} 3"));
        assert!(out.contains("gproxy_quota_used{scope=\"user\",scope_id=\"9\"} 12.5"));
        // every family carries a TYPE line
        assert!(out.contains("# TYPE gproxy_upstream_latency_ms histogram"));
    }
}
