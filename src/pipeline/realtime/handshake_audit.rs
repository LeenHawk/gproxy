//! `upstream_requests` audit rows for native Realtime WebSocket handshakes.

use bytes::Bytes;
use serde_json::Value;

use crate::app::AppState;
use crate::http::client::{ClientError, ConduitSocket};
use crate::http::redaction::{headers_json, redact_query, warn_unless_redacted};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::store::persistence::records::UpstreamRequestInput;
use crate::util::time::unix_now;

pub(super) struct HandshakeAudit {
    request_id: String,
    at: i64,
    provider_id: i64,
    credential_id: i64,
    url: String,
    headers_json: Value,
    redact: bool,
}

impl HandshakeAudit {
    pub(super) fn capture(
        state: &AppState,
        ctx: &RequestCtx,
        candidate: &Candidate,
        request: &http::Request<Bytes>,
    ) -> Option<Self> {
        let settings = state.cp().log_settings.clone();
        if !settings.enable_upstream_log {
            return None;
        }
        let redact = warn_unless_redacted(&settings);
        Some(Self {
            request_id: ctx.request_id.clone(),
            at: unix_now(),
            provider_id: candidate.provider.id,
            credential_id: candidate.credential.id,
            url: audit_url(request.uri(), redact),
            headers_json: headers_json(request.headers(), redact),
            redact,
        })
    }

    pub(super) async fn record(
        self,
        state: &AppState,
        result: Result<Box<dyn ConduitSocket>, ClientError>,
        latency_ms: i64,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        let (status, response_body) = match &result {
            Ok(_) => (101, None),
            Err(error) => (0, Some(redact_error(error, self.redact))),
        };
        let input = UpstreamRequestInput {
            request_id: self.request_id.clone(),
            at: self.at,
            provider_id: Some(self.provider_id),
            credential_id: Some(self.credential_id),
            url: self.url,
            method: "GET".to_owned(),
            status,
            latency_ms,
            headers_json: Some(self.headers_json),
            body: None,
            response_body,
        };
        if let Err(error) = state.persistence.append_upstream_request(input).await {
            tracing::warn!(
                request_id = %self.request_id,
                provider_id = self.provider_id,
                credential_id = self.credential_id,
                error = %error,
                "realtime handshake audit write failed"
            );
        }
        result
    }
}

fn audit_url(uri: &http::Uri, redact: bool) -> String {
    let url = uri.to_string();
    match uri.query() {
        Some(query) => {
            let prefix = &url[..url.len() - query.len()];
            format!("{prefix}{}", redact_query(query, redact))
        }
        None => url,
    }
}

fn redact_error(error: &ClientError, redact: bool) -> String {
    let message = error.to_string();
    if !redact {
        return message;
    }
    redact_embedded_url_queries(&message)
}

fn redact_embedded_url_queries(message: &str) -> String {
    let mut output = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = ["http://", "https://", "ws://", "wss://"]
        .into_iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"'))
            .unwrap_or(tail.len());
        let url = &tail[..end];
        match url.find('?') {
            Some(query_start) => {
                output.push_str(&url[..=query_start]);
                output.push_str(&redact_query(&url[query_start + 1..], true));
            }
            None => output.push_str(url),
        }
        rest = &tail[end..];
    }
    output.push_str(rest);
    output
}
