//! What the account has left, on whichever billing meter it belongs to.
//!
//! An enterprise seat reads `POST /v2/billing/meter/get-enterprise-user-usage`
//! with an empty body and gets one `{limitNum, credit, cycleResetTime}`
//! record. A personal account reads
//! `POST /v2/billing/meter/get-user-resource` with the plugin's own captured
//! query — product code `p_tcaca`, statuses 0 and 3, and a package window
//! that reaches a century out — and gets a list of packages, each with
//! precise cycle counters and an ISO cycle end (v3 `workbuddy/quota.rs`).
//! The list nests at a depth that varies with how far the gateway wrapped
//! it, so three pointers are tried.
//!
//! There is no `QuotaModel`: which packages an account holds is only knowable
//! once the meter has answered, and nothing on the credential could state the
//! window — the same reading `geminicli` and `antigravity` take.

use super::auth::{authorize, fact};
use super::config::{ID, WorkBuddyConfig, base_url};
use super::{json_headers, send, unix_now_ms};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, QuotaAllowance, QuotaEntry,
    QuotaQuery, QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use http::Method;
use rust_decimal::Decimal;
use serde_json::{Value, json};

/// The one window an enterprise seat has.
pub const ENTERPRISE_DIMENSION: &str = "enterprise";
/// The product code the plugin's own meter query names.
const PRODUCT_CODE: &str = "p_tcaca";
/// The package end range the plugin asks for, in seconds (v3 `quota.rs`).
const PACKAGE_WINDOW_SECS: i64 = 101 * 365 * 86_400;

fn enterprise_seat(credential: &CredentialView<'_>) -> bool {
    fact(credential, "enterprise_id").is_some()
}

fn decimal(value: Option<&Value>) -> Option<Decimal> {
    match value? {
        Value::Number(number) => number.to_string().parse().ok(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}

fn used_percent(used: Decimal, limit: Decimal) -> Option<Decimal> {
    (!limit.is_zero()).then(|| used / limit * Decimal::from(100))
}

/// `YYYY-MM-DD HH:MM:SS`, the format the billing endpoint expects.
fn format_time(seconds: i64) -> Option<String> {
    let stamp = time::OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        stamp.year(),
        u8::from(stamp.month()),
        stamp.day(),
        stamp.hour(),
        stamp.minute(),
        stamp.second(),
    ))
}

fn personal_body(now_secs: i64) -> Result<Vec<u8>, ChannelError> {
    let begin = format_time(now_secs)
        .ok_or_else(|| ChannelError::InvalidConfig("the system clock is unusable".into()))?;
    let end = format_time(now_secs.saturating_add(PACKAGE_WINDOW_SECS))
        .ok_or_else(|| ChannelError::InvalidConfig("the system clock is unusable".into()))?;
    Ok(json!({
        "PageNumber": 1,
        "PageSize": 100,
        "ProductCode": PRODUCT_CODE,
        "Status": [0, 3],
        "PackageEndTimeRangeBegin": begin,
        "PackageEndTimeRangeEnd": end,
    })
    .to_string()
    .into_bytes())
}

pub(super) fn entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let raw: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::InvalidResponse(format!("{ID} quota: {error}")))?;
    if let Some(accounts) = raw
        .pointer("/data/Response/Data/Accounts")
        .or_else(|| raw.pointer("/data/data/Response/Data/Accounts"))
        .or_else(|| raw.pointer("/Response/Data/Accounts"))
        .and_then(Value::as_array)
    {
        return Ok(personal(accounts));
    }
    let data = raw
        .pointer("/data/data")
        .or_else(|| raw.get("data"))
        .unwrap_or(&raw);
    Ok(enterprise(data).into_iter().collect())
}

fn window(
    id: String,
    label: Option<String>,
    used: Decimal,
    limit: Decimal,
    period_end_ms: Option<i64>,
) -> QuotaEntry {
    QuotaEntry {
        id: id.clone(),
        source_id: id,
        label,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            used: Some(used),
            limit: Some(limit),
            remaining: Some((limit - used).max(Decimal::ZERO)),
            used_percent: used_percent(used, limit),
            unlimited: None,
            unit: None,
            // The meter dates the reset and nothing else.
            period_start_ms: None,
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}

fn personal(accounts: &[Value]) -> Vec<QuotaEntry> {
    accounts
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            let limit = decimal(resource.get("CycleCapacitySizePrecise")).unwrap_or(Decimal::ZERO);
            let left = decimal(resource.get("CycleCapacityRemainPrecise")).unwrap_or(Decimal::ZERO);
            let used = (limit - left).max(Decimal::ZERO);
            let id = text(resource, "PackageCode")
                .or_else(|| text(resource, "ResourceId"))
                .map(str::to_owned)
                .unwrap_or_else(|| format!("resource_{index}"));
            let label = text(resource, "PackageCode").map(str::to_owned);
            window(
                id,
                label,
                used,
                limit,
                text(resource, "CycleEndTime").and_then(iso_to_ms),
            )
        })
        .collect()
}

fn enterprise(data: &Value) -> Option<QuotaEntry> {
    let limit = decimal(data.get("limitNum"))?;
    let used = decimal(data.get("credit")).unwrap_or(Decimal::ZERO);
    Some(window(
        ENTERPRISE_DIMENSION.to_owned(),
        Some("enterprise".into()),
        used,
        limit,
        text(data, "cycleResetTime").and_then(iso_to_ms),
    ))
}

impl QuotaQuery for super::WorkBuddy {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = WorkBuddyConfig::from_view(context.provider)?;
            let seat = enterprise_seat(&context.credential);
            let path = if seat {
                "/v2/billing/meter/get-enterprise-user-usage"
            } else {
                "/v2/billing/meter/get-user-resource"
            };
            let body = if seat {
                b"{}".to_vec()
            } else {
                personal_body(unix_now_ms() / 1_000)?
            };
            let mut headers = json_headers();
            authorize(&mut headers, &context.credential)?;
            super::auth::identity(&mut headers, &config, None)?;
            let (status, _, reply) = send(
                context.client,
                Method::POST,
                &format!("{}{path}", base_url(context.provider)),
                headers,
                Some(body),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: reply,
                });
            }
            let entries = entries(&reply)?;
            if entries.is_empty() {
                return Err(ChannelError::InvalidResponse(format!(
                    "{ID} quota: the meter reported no readings"
                )));
            }
            Ok(QuotaSnapshot {
                observed_at_ms: unix_now_ms(),
                entries,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_personal_package_becomes_a_window_keyed_by_its_code() {
        let body = br#"{"data":{"Response":{"Data":{"Accounts":[
          {"PackageCode":"pkg_basic","CycleCapacitySizePrecise":1000,
           "CycleCapacityRemainPrecise":"250.5","CycleEndTime":"2026-09-01T00:00:00Z"}
        ]}}}}"#;
        let entries = entries(body).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "pkg_basic");
        let QuotaValue::Window(window) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used, Some("749.5".parse().unwrap()));
        assert_eq!(window.limit, Some("1000".parse().unwrap()));
        assert_eq!(window.used_percent, Some("74.95".parse().unwrap()));
        assert_eq!(window.period_end_ms, Some(1_788_220_800_000));
    }

    #[test]
    fn an_enterprise_meter_becomes_the_one_window_it_has() {
        let read = entries(br#"{"data":{"data":{"limitNum":500,"credit":"120"}}}"#).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].id, ENTERPRISE_DIMENSION);
        let QuotaValue::Window(window) = &read[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used_percent, Some(Decimal::from(24)));
        assert_eq!(window.remaining, Some(Decimal::from(380)));
        assert!(entries(br#"{"data":{}}"#).unwrap().is_empty());
    }

    #[test]
    fn the_meter_query_states_its_window_the_way_the_plugin_does() {
        assert_eq!(
            format_time(1_767_225_600).as_deref(),
            Some("2026-01-01 00:00:00")
        );
        let body = String::from_utf8(personal_body(1_767_225_600).unwrap()).unwrap();
        assert!(body.contains(r#""ProductCode":"p_tcaca""#), "{body}");
        assert!(
            body.contains(r#""PackageEndTimeRangeBegin":"2026-01-01 00:00:00""#),
            "{body}"
        );
    }
}
