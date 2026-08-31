//! Antigravity quota riding `POST {base}/v1internal:fetchAvailableModels`
//! (empty JSON body): each entry under `models` may carry a `quotaInfo` with
//! `remainingFraction` (fraction LEFT in [0, 1]) and an ISO-8601 `resetTime`.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde_json::Value;

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let access = super::auth::access_token(secret)?;
    let uri = super::prepare::endpoint_uri(
        settings,
        "gemini_list_models",
        "/v1internal:fetchAvailableModels",
        None,
    )?;
    http::Request::post(uri)
        .header(
            AUTHORIZATION,
            http::HeaderValue::from_str(&format!("Bearer {access}"))
                .map_err(|error| ChannelError::Secret(error.to_string()))?,
        )
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, super::prepare::USER_AGENT_VALUE)
        .body(Bytes::from_static(b"{}"))
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(raw) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let Some(models) = raw.get("models").and_then(Value::as_object) else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|(model_id, model)| {
            let quota = model.get("quotaInfo").filter(|value| value.is_object())?;
            Some(QuotaObservation {
                window_key: model_id.clone(),
                period_start: None,
                period_end: quota
                    .get("resetTime")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|reset| !reset.is_empty())
                    .and_then(crate::shared::quota::iso_to_unix),
                used_percent: quota
                    .get("remainingFraction")
                    .and_then(Value::as_f64)
                    .and_then(crate::shared::quota::remaining_fraction_to_used_percent),
                upstream_used: None,
                upstream_limit: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn models_with_quota_info_become_observations() {
        let body = br#"{"models":{
          "gemini-3.1-pro":{"quotaInfo":{"remainingFraction":0.8,"resetTime":"2026-08-01T12:00:00Z"}},
          "no-quota":{},
          "null-quota":{"quotaInfo":null}
        }}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].window_key, "gemini-3.1-pro");
        assert_eq!(observed[0].used_percent, Some("20".parse().unwrap()));
        assert_eq!(observed[0].period_end, Some(1_785_585_600));
        assert_eq!(observed[0].period_start, None);
    }
}
