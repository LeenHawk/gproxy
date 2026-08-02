//! Audit persistence shared by non-provider outbound utility calls.

use std::sync::Arc;

use crate::app::snapshot::LogSettings;
use crate::http::redaction::{body_string, redact_query, warn_unless_redacted};
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::UpstreamRequestInput;

#[derive(Clone)]
pub(crate) struct UtilityAudit {
    persistence: Arc<dyn PersistenceBackend>,
    request_id: String,
    redact: bool,
    capture_body: bool,
}

pub(crate) enum AuditBody<'a> {
    Response(&'a [u8]),
    Error(&'a str),
}

pub(crate) struct UtilityAuditCall<'a> {
    pub at: i64,
    pub url: &'a str,
    pub method: &'a str,
    pub status: i64,
    pub latency_ms: i64,
    pub body: Option<AuditBody<'a>>,
    /// Signed download URLs must remain safe even when general redaction is off.
    pub force_query_redaction: bool,
}

impl UtilityAudit {
    pub(crate) fn new(
        persistence: Arc<dyn PersistenceBackend>,
        request_id: String,
        settings: &LogSettings,
    ) -> Option<Self> {
        settings.enable_upstream_log.then(|| Self {
            persistence,
            request_id,
            redact: warn_unless_redacted(settings),
            capture_body: settings.enable_upstream_log_body,
        })
    }

    pub(crate) async fn record(&self, call: UtilityAuditCall<'_>) {
        let redact_query = self.redact || call.force_query_redaction;
        let response_body = self.capture_body.then(|| match call.body {
            Some(AuditBody::Response(body)) => Some(body_string(body, self.redact)),
            Some(AuditBody::Error(error)) => {
                let error = if redact_query {
                    crate::http::telemetry::redact_url_query(error)
                } else {
                    std::borrow::Cow::Borrowed(error)
                };
                Some(body_string(error.as_bytes(), self.redact))
            }
            None => None,
        });
        let input = UpstreamRequestInput {
            request_id: self.request_id.clone(),
            at: call.at,
            provider_id: None,
            credential_id: None,
            url: audit_url(call.url, redact_query),
            method: call.method.to_owned(),
            status: call.status,
            latency_ms: call.latency_ms,
            headers_json: None,
            body: None,
            response_body: response_body.flatten(),
        };
        if let Err(error) = self.persistence.append_upstream_request(input).await {
            tracing::warn!(
                request_id = %self.request_id,
                error = %error,
                "utility upstream audit write failed"
            );
        }
    }
}

fn audit_url(url: &str, redact: bool) -> String {
    let Some((prefix, query)) = url.split_once('?') else {
        return url.to_owned();
    };
    format!("{prefix}?{}", redact_query(query, redact))
}
