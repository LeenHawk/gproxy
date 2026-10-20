use super::*;
use crate::convert::upstream_error;

pub(in super::super) struct Failure {
    pub status: u16,
    pub detail: Value,
}

impl Failure {
    pub fn new(status: u16, message: impl std::fmt::Display) -> Self {
        Self::detail(status, json!({"message": message.to_string()}))
    }

    pub fn transport(error: impl std::fmt::Display) -> Self {
        Self::new(502, error)
    }

    pub fn core(error: CoreError) -> Self {
        if let CoreError::Transform(error) = &error
            && let Some(detail) = upstream_error::detail(error)
        {
            return Self::detail(upstream_error::status(&detail), detail);
        }
        let status = match &error {
            CoreError::BudgetExhausted { .. } => 429,
            CoreError::Forbidden(_) => 403,
            CoreError::DeadlineExceeded => 504,
            CoreError::Cancelled => 499,
            CoreError::Transform(error)
                if matches!(
                    error.kind(),
                    gproxy_protocol::transform::TransformErrorKind::Host
                        | gproxy_protocol::transform::TransformErrorKind::InvalidResult
                ) =>
            {
                502
            }
            CoreError::InvalidTarget(_) | CoreError::Transform(_) | CoreError::Rewrite(_) => 400,
            _ => 502,
        };
        Self::new(status, error)
    }

    pub fn rejected(status: u16, bytes: &[u8]) -> Self {
        let value = serde_json::from_slice::<Value>(bytes).ok();
        let detail = value
            .as_ref()
            .and_then(|value| value.get("error"))
            .filter(|error| error.is_object())
            .cloned()
            .unwrap_or_else(
                || json!({"message": format!("upstream rejected generation ({status})")}),
            );
        Self::detail(status, detail)
    }

    pub fn event(event: &Value) -> Self {
        let detail = upstream_error::unwrap(event.clone());
        let status = upstream_error::status(&detail);
        Self::detail(status, detail)
    }

    fn detail(status: u16, mut detail: Value) -> Self {
        let (kind, code) = match status {
            401 => ("authentication_error", "invalid_api_key"),
            402 => ("billing_error", "billing_error"),
            403 => ("permission_error", "permission_denied"),
            409 => ("conflict_error", "conflict_error"),
            413 => ("request_too_large", "request_too_large"),
            429 => ("rate_limit_error", "rate_limit_exceeded"),
            400..=499 => ("invalid_request_error", "invalid_request"),
            _ => ("server_error", "server_error"),
        };
        if detail["type"].is_null() {
            detail["type"] = kind.into();
        }
        if detail["code"].is_number() {
            // The Responses WS schema uses strings; retain the vendor's
            // numeric value too (notably Google's HTTP-style error codes).
            detail["upstream_code"] = detail["code"].clone();
            detail["code"] = detail["code"].to_string().into();
        }
        if detail["code"].is_null() {
            detail["code"] = code.into();
        }
        if detail["message"].is_null() {
            detail["message"] = format!("upstream generation failed ({status})").into();
        }
        Self { status, detail }
    }

    pub fn message(&self) -> &str {
        self.detail["message"]
            .as_str()
            .unwrap_or("upstream generation failed")
    }
}
