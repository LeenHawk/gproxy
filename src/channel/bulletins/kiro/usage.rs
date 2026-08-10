//! Kiro per-credential usage — captured `AmazonCodeWhispererService.GetUsageLimits`
//! Smithy op on the management host: `POST https://management.{region}.kiro.dev/
//! ?profileArn=…&origin=KIRO_CLI&isEmailRequired=true`, body
//! `{"profileArn","origin","isEmailRequired":true}`. Returns the subscription
//! title + a usage breakdown (current vs limit, fractional credit units) with a
//! unix reset.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{AMZ_JSON, ORIGIN, TARGET_USAGE, UA_MANAGEMENT, auth, management_base};
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{
    UsageSnapshot, UsageWindow, UsageWindowDescriptor, UsageWindowMeter, UsageWindowScope,
};

/// Build the captured Kiro CLI `GetUsageLimits` request. `None` when the
/// credential has no profileArn (the API requires it).
pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = auth::access_token(secret)?;
    let Some(profile_arn) = auth::profile_arn(secret, settings) else {
        return Ok(None);
    };

    let path = format!(
        "/?profileArn={}&origin={ORIGIN}&isEmailRequired=true",
        pct(profile_arn)
    );
    let uri = join_url(&management_base(settings), &path, None)?;
    let body = serde_json::to_vec(&json!({
        "profileArn": profile_arn,
        "origin": ORIGIN,
        "isEmailRequired": true,
    }))
    .map_err(|e| ChannelError::Build(format!("kiro usage body: {e}")))?;

    let mut req = build_request(Method::POST, uri, HeaderMap::new(), Bytes::from(body))?;
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let invocation = HeaderValue::from_str(&crate::util::rand::uuid_v4())
        .map_err(|e| ChannelError::Build(format!("bad invocation id: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("*/*"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    h.insert(USER_AGENT, HeaderValue::from_static(UA_MANAGEMENT));
    h.insert(
        HeaderName::from_static("x-amz-user-agent"),
        HeaderValue::from_static(UA_MANAGEMENT),
    );
    h.insert(
        HeaderName::from_static("x-amz-target"),
        HeaderValue::from_static(TARGET_USAGE),
    );
    h.insert(
        HeaderName::from_static("x-amzn-codewhisperer-optout"),
        HeaderValue::from_static("false"),
    );
    h.insert(
        HeaderName::from_static("amz-sdk-request"),
        HeaderValue::from_static("attempt=1; max=3"),
    );
    h.insert(HeaderName::from_static("amz-sdk-invocation-id"), invocation);
    Ok(Some(req))
}

/// Parse the `getUsageLimits` body (`UsageLimitsResponse`) into a snapshot.
pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let resp: UsageLimitsResponse = serde_json::from_value(raw.clone()).ok()?;

    let single = resp.usage_breakdown_list.len() == 1;
    let windows = resp
        .usage_breakdown_list
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = b
                .resource_type
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(stable_key)
                .unwrap_or_else(|| {
                    if single {
                        "agentic_request".to_string()
                    } else {
                        format!("usage_{i}")
                    }
                });
            let mut w = UsageWindow {
                name,
                used: b.current_usage_with_precision,
                limit: b.usage_limit_with_precision,
                ..Default::default()
            };
            if let Some(reset) = b
                .next_date_reset
                .as_ref()
                .and_then(epoch_seconds)
                .or_else(|| resp.next_date_reset.as_ref().and_then(epoch_seconds))
            {
                w = w.resets_unix(reset);
            }
            w
        })
        .collect();

    Some(UsageSnapshot {
        plan: resp
            .subscription_info
            .and_then(|s| s.subscription_title)
            .filter(|s| !s.is_empty()),
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
    let resource_type = snapshot
        .raw
        .get("usageBreakdownList")
        .and_then(Value::as_array)
        .and_then(|items| items.get(index))
        .and_then(|item| item.get("resourceType"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let scope = match resource_type {
        Some(value) if stable_key(value) == "agentic_request" => UsageWindowScope::All,
        Some(value) => UsageWindowScope::Feature {
            feature: value.to_owned(),
        },
        None if window.name == "agentic_request" => UsageWindowScope::All,
        None => UsageWindowScope::Unknown,
    };
    UsageWindowDescriptor::from_window(window)
        .scope(scope)
        .meter(UsageWindowMeter::Credits)
}

fn stable_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "usage".to_owned()
    } else {
        out.to_owned()
    }
}

fn epoch_seconds(value: &Value) -> Option<i64> {
    let value = match value {
        Value::Number(value) => value.as_f64()?,
        Value::String(value) => value.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    let seconds = if value.abs() >= 1_000_000_000_000.0 {
        value / 1000.0
    } else {
        value
    };
    Some(seconds as i64)
}

/// Percent-encode a query value, leaving the RFC 3986 unreserved set verbatim
/// (`profileArn` carries `:` and `/`).
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageLimitsResponse {
    #[serde(default)]
    next_date_reset: Option<Value>,
    #[serde(default)]
    subscription_info: Option<SubscriptionInfo>,
    #[serde(default)]
    usage_breakdown_list: Vec<UsageBreakdown>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionInfo {
    #[serde(default)]
    subscription_title: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageBreakdown {
    #[serde(default)]
    resource_type: Option<String>,
    #[serde(default)]
    current_usage_with_precision: Option<f64>,
    #[serde(default)]
    usage_limit_with_precision: Option<f64>,
    #[serde(default)]
    next_date_reset: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_get_usage_limits_request() {
        let secret = json!({
            "access_token": "t",
            "profile_arn": "arn:aws:codewhisperer:us-east-1:1:profile/x",
        });
        let req = request(&secret, &json!({})).unwrap().unwrap();
        assert_eq!(req.method(), Method::POST);
        assert_eq!(
            req.uri().to_string(),
            "https://management.us-east-1.kiro.dev/?profileArn=\
             arn%3Aaws%3Acodewhisperer%3Aus-east-1%3A1%3Aprofile%2Fx&origin=KIRO_CLI&isEmailRequired=true"
        );
        assert_eq!(
            req.headers().get("x-amz-target").unwrap(),
            "AmazonCodeWhispererService.GetUsageLimits"
        );
        let body: Value = serde_json::from_slice(req.body()).unwrap();
        assert_eq!(
            body,
            json!({"profileArn":"arn:aws:codewhisperer:us-east-1:1:profile/x","origin":"KIRO_CLI","isEmailRequired":true})
        );
    }

    #[test]
    fn no_usage_without_profile_arn() {
        assert!(
            request(&json!({ "access_token": "t" }), &json!({}))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn parses_usage_limits() {
        let body = Bytes::from_static(
            br#"{
              "nextDateReset": 1735689600,
              "subscriptionInfo": {"subscriptionTitle": "KIRO PRO+"},
              "usageBreakdownList": [
                {"currentUsage": 120, "currentUsageWithPrecision": 120.5,
                 "usageLimit": 1000, "usageLimitWithPrecision": 1000.0, "nextDateReset": 1735689600}
              ]
            }"#,
        );
        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.plan.as_deref(), Some("KIRO PRO+"));
        assert_eq!(snap.windows.len(), 1);
        assert_eq!(snap.windows[0].name, "agentic_request");
        assert_eq!(snap.windows[0].used, Some(120.5));
        assert_eq!(snap.windows[0].limit, Some(1000.0));
        assert_eq!(snap.windows[0].resets_at_unix, Some(1735689600));
    }
}
