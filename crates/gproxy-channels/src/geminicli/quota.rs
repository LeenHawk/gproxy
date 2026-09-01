//! Gemini CLI per-credential quota — `POST {base}/v1internal:retrieveUserQuota`
//! with `{"project": <id>}`, the same Code Assist endpoint the model list
//! reads. One bucket per model/token-type pair: `remainingFraction` is the
//! fraction LEFT in [0, 1], `resetTime` an ISO-8601 timestamp, and the limit
//! arrives as `quotaAmount`, `maxAmount`, or `limit` depending on server
//! generation — numbers or numeric strings.

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let access = super::auth::access_token(secret)?;
    let Ok(project) = super::auth::project_id(secret) else {
        return Ok(None);
    };
    let uri = super::prepare::endpoint_uri(
        settings,
        "gemini_list_models",
        "/v1internal:retrieveUserQuota",
        None,
    )?;
    let mut headers = http::HeaderMap::new();
    super::prepare::apply_headers(&mut headers, access, "gemini-2.5-pro", true)?;
    let mut request = http::Request::post(uri)
        .body(Bytes::from(json!({"project": project}).to_string()))
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(Some(request))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_slice::<UserQuota>(body) else {
        return Vec::new();
    };
    payload
        .buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| bucket.observation(index))
        .collect()
}

#[derive(Deserialize)]
struct UserQuota {
    #[serde(default)]
    buckets: Vec<Bucket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bucket {
    model_id: Option<String>,
    token_type: Option<String>,
    remaining_fraction: Option<f64>,
    remaining_amount: Option<Value>,
    #[serde(default, alias = "quotaAmount", alias = "maxAmount")]
    limit: Option<Value>,
    reset_time: Option<String>,
}

impl Bucket {
    fn observation(&self, index: usize) -> QuotaObservation {
        let fraction = self.remaining_fraction.filter(|value| value.is_finite());
        let remaining = self
            .remaining_amount
            .as_ref()
            .and_then(crate::shared::quota::decimal);
        let explicit = self.limit.as_ref().and_then(crate::shared::quota::decimal);
        let derived = remaining.zip(fraction).and_then(|(remaining, fraction)| {
            if remaining < Decimal::ZERO || fraction <= 0.0 || fraction > 1.0 {
                return None;
            }
            remaining.checked_div(Decimal::try_from(fraction).ok()?)
        });
        let limit = explicit.or(derived).filter(|value| *value >= Decimal::ZERO);
        let used = limit.and_then(|limit| match (remaining, fraction) {
            (Some(remaining), _) => Some((limit - remaining).max(Decimal::ZERO)),
            (None, Some(fraction)) => {
                let fraction = Decimal::try_from(fraction.clamp(0.0, 1.0)).ok()?;
                limit
                    .checked_mul(Decimal::ONE - fraction)
                    .map(|used| used.max(Decimal::ZERO))
            }
            (None, None) => None,
        });
        QuotaObservation {
            window_key: window_key(self.model_id.as_deref(), self.token_type.as_deref(), index),
            label: None,
            period_start: None,
            period_end: self
                .reset_time
                .as_deref()
                .and_then(crate::shared::quota::iso_to_unix),
            used_percent: fraction
                .and_then(crate::shared::quota::remaining_fraction_to_used_percent),
            upstream_used: used,
            upstream_limit: limit,
        }
    }
}

fn window_key(model: Option<&str>, token_type: Option<&str>, index: usize) -> String {
    let model = model.map(str::trim).filter(|value| !value.is_empty());
    let token_type = token_type.map(str::trim).filter(|value| !value.is_empty());
    match (model, token_type) {
        (Some(model), Some(token_type)) => {
            format!("{}:{}", component(model), component(token_type))
        }
        (Some(model), None) => format!("{}:unknown", component(model)),
        (None, Some(token_type)) => format!("unknown:{}", component(token_type)),
        (None, None) => format!("bucket_{index}"),
    }
}

/// Like the shared slug but keeps `-`, `_`, and `.` — model ids stay readable.
fn component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let output = output.trim_matches('_');
    if output.is_empty() {
        "unknown".into()
    } else {
        output.into()
    }
}

#[cfg(test)]
mod tests {
    use super::parse_probe;

    #[test]
    fn buckets_become_observations_with_derived_amounts() {
        let body = br#"{"buckets":[
          {"modelId":"gemini-2.5-pro","tokenType":"REQUESTS","remainingFraction":0.75,
           "resetTime":"2026-06-22T16:01:15Z"},
          {"modelId":"gemini-2.5-flash","tokenType":"REQUESTS","remainingFraction":0.5,
           "remainingAmount":"50"}
        ]}"#;
        let observed = parse_probe(http::StatusCode::OK, body);
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].window_key, "gemini-2.5-pro:requests");
        assert_eq!(observed[0].used_percent, Some("25".parse().unwrap()));
        assert_eq!(observed[0].period_end, Some(1_782_144_075));
        assert_eq!(observed[1].window_key, "gemini-2.5-flash:requests");
        assert_eq!(observed[1].upstream_limit, Some("100".parse().unwrap()));
        assert_eq!(observed[1].upstream_used, Some("50".parse().unwrap()));
    }
}
