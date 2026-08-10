//! Antigravity quota from `fetchAvailableModels` model metadata.

use bytes::Bytes;
use http::{HeaderMap, Method, Request, StatusCode};
use serde_json::{Value, json};

use super::auth;
use crate::channel::http_util::{build_request, exact_url, join_url};
use crate::channel::usage::{
    UsageSnapshot, UsageWindow, UsageWindowDescriptor, UsageWindowMeter, UsageWindowScope,
};
use crate::channel::{ChannelError, settings};

const PATH: &str = "/v1internal:fetchAvailableModels";

pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = auth::access_token(secret)?;
    let uri = match settings::endpoint_by_key(settings, "usage", "") {
        Some(url) => exact_url(&url, None)?,
        None => {
            let base = settings
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(auth::BASE_URL);
            join_url(base, PATH, None)?
        }
    };
    let body = Bytes::from(serde_json::to_vec(&json!({})).expect("empty object serializes"));
    let mut request = build_request(Method::POST, uri, HeaderMap::new(), body)?;
    auth::apply(&mut request, access_token)?;
    Ok(Some(request))
}

pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let mut windows = Vec::new();
    if let Some(models) = raw.get("models").and_then(Value::as_object) {
        for (model_id, model) in models {
            let Some(quota) = model.get("quotaInfo").and_then(Value::as_object) else {
                continue;
            };
            let used_percent = quota
                .get("remainingFraction")
                .and_then(Value::as_f64)
                .map(|remaining| ((1.0 - remaining) * 100.0).clamp(0.0, 100.0));
            let mut window = UsageWindow {
                name: model_id.clone(),
                used_percent,
                ..Default::default()
            };
            if let Some(reset) = quota
                .get("resetTime")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|reset| !reset.is_empty())
            {
                window = window.resets_iso(reset);
            }
            windows.push(window);
        }
    }
    Some(UsageSnapshot {
        plan: None,
        windows,
        credits: None,
        rate_limit_reset_credits: None,
        raw,
    })
}

pub(super) fn describe(snapshot: &UsageSnapshot, index: usize) -> UsageWindowDescriptor {
    let Some(window) = snapshot.windows.get(index) else {
        return UsageWindowDescriptor::from_window(&UsageWindow {
            name: format!("window_{index}"),
            ..Default::default()
        });
    };
    UsageWindowDescriptor::from_window(window)
        .scope(UsageWindowScope::Models {
            models: vec![window.name.clone()],
        })
        .meter(UsageWindowMeter::Opaque)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_daily_available_models_without_project() {
        let request = request(&json!({"access_token": "token"}), &Value::Null)
            .unwrap()
            .unwrap();
        assert_eq!(
            request.uri().to_string(),
            "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
        );
        assert_eq!(request.body().as_ref(), b"{}");
    }

    #[test]
    fn parses_model_quota_info() {
        let body = Bytes::from_static(
            br#"{"models":{"gemini-3.1-pro":{"quotaInfo":{"remainingFraction":0.8,"resetTime":"2026-08-01T12:00:00Z"}},"no-quota":{},"null-quota":{"quotaInfo":null}}}"#,
        );
        let snapshot = parse(StatusCode::OK, &body).expect("usage snapshot");
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].name, "gemini-3.1-pro");
        assert!((snapshot.windows[0].used_percent.unwrap() - 20.0).abs() < 1e-9);
        assert_eq!(
            snapshot.windows[0].resets_at.as_deref(),
            Some("2026-08-01T12:00:00Z")
        );
    }
}
