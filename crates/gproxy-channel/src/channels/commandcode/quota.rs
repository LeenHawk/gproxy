//! CLI fetchUsageData/projectUsageView: whoami selects the organization;
//! credits and subscriptions are fetched together, then usage/summary receives
//! the subscription's currentPeriodStart as `since`. Amounts are reported
//! separately: remaining credits are not a subscription spending limit.
//!
//! These observations do not add local blocking dimensions. Extra credits can
//! keep a subscription usable past its window, and coverage is plan-specific.
use super::{
    CommandCode,
    config::{api_key, base_url},
    request,
};
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaBalance, QuotaEntry,
    QuotaQuery, QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{decimal, require_success, send};
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;

/// WindowLimitMeter compares resetAt directly with Date.now(); subscription
/// boundaries and organization limits are date strings. Keep either wire form.
fn timestamp(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| {
            time::OffsetDateTime::parse(
                value.as_str()?,
                &time::format_description::well_known::Rfc3339,
            )
            .ok()
            .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
        })
        .filter(|timestamp| *timestamp > 0)
}

fn allowance(used: Option<Decimal>, limit: Option<Decimal>, unit: &str) -> QuotaAllowance {
    QuotaAllowance {
        used,
        limit,
        remaining: used
            .zip(limit)
            .map(|(used, limit)| (limit - used).max(Decimal::ZERO)),
        used_percent: used
            .zip(limit)
            .filter(|(_, limit)| *limit > Decimal::ZERO)
            .map(|(used, limit)| used / limit * Decimal::from(100)),
        unit: Some(unit.into()),
        ..Default::default()
    }
}

fn entry(
    id: &str,
    source: &str,
    label: String,
    subject: QuotaSubject,
    scope: QuotaScope,
    value: QuotaValue,
) -> QuotaEntry {
    QuotaEntry {
        id: id.into(),
        source_id: source.into(),
        label: Some(label),
        subject,
        model_scope: scope,
        value,
    }
}

fn entries(
    whoami: &Value,
    payload: &Value,
    subscription: &Value,
    summary: &Value,
    subject: QuotaSubject,
) -> Result<Vec<QuotaEntry>, ChannelError> {
    let credits = payload
        .get("credits")
        .filter(|v| v.is_object())
        .ok_or_else(|| ChannelError::InvalidResponse("Command Code: missing credits".into()))?;
    let mut entries = Vec::new();
    for (field, label) in [
        ("monthlyCredits", "Monthly credits"),
        ("purchasedCredits", "Purchased credits"),
        ("freeCredits", "Free credits"),
    ] {
        if let Some(remaining) = credits.get(field).and_then(decimal) {
            entries.push(entry(
                field,
                "commandcode_credits",
                label.into(),
                subject,
                QuotaScope::All,
                QuotaValue::Balance(QuotaBalance {
                    remaining: Some(remaining),
                    unit: Some("USD".into()),
                }),
            ));
        }
    }
    if payload
        .pointer("/windowLimits/limited")
        .and_then(Value::as_bool)
        == Some(true)
    {
        for (field, label) in [("fiveHour", "5-hour usage"), ("weekly", "Weekly usage")] {
            let Some(window) = payload["windowLimits"].get(field).filter(|v| !v.is_null()) else {
                continue;
            };
            let mut value = allowance(
                window.get("used").and_then(decimal),
                window.get("cap").and_then(decimal),
                "USD",
            );
            value.period_end_ms = timestamp(&window["resetAt"]);
            value.reset_behavior = QuotaResetBehavior::Periodic;
            entries.push(entry(
                field,
                "commandcode_windows",
                label.into(),
                subject,
                QuotaScope::Unknown,
                QuotaValue::Window(value),
            ));
        }
    }

    // The API states whether the summary covers the subscription billing
    // period. Do not attach subscription dates to a fallback/history summary.
    let billing_period = summary["periodBasis"] == "billing-period";
    let (start, end) = if billing_period {
        (
            timestamp(&subscription["currentPeriodStart"]),
            timestamp(&subscription["currentPeriodEnd"]),
        )
    } else {
        (None, None)
    };
    let plan = [
        subscription["planId"].as_str(),
        subscription["status"].as_str(),
    ]
    .into_iter()
    .flatten()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    for (id, field, label, unit) in [
        (
            "usage_cost",
            "totalCost",
            if billing_period {
                "Billing period spend"
            } else {
                "Reported spend"
            },
            "USD",
        ),
        (
            "usage_requests",
            "totalCount",
            if billing_period {
                "Billing period requests"
            } else {
                "Reported requests"
            },
            "requests",
        ),
    ] {
        let Some(used) = summary.get(field).and_then(decimal) else {
            continue;
        };
        let mut value = allowance(Some(used), None, unit);
        value.period_start_ms = start;
        value.period_end_ms = end;
        let label = if plan.is_empty() {
            label.into()
        } else {
            format!("{label} · {plan}")
        };
        entries.push(entry(
            id,
            "commandcode_usage",
            label,
            subject,
            QuotaScope::All,
            QuotaValue::Budget(value),
        ));
    }

    // CLI OrgLimitsSection reads the actual organization/model spend caps.
    for limit in whoami
        .get("orgLimits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let scope = limit["scope"].as_str().unwrap_or_default();
        let model = limit["model"].as_str().unwrap_or_default();
        let interval = limit["resetInterval"].as_str().unwrap_or_default();
        let id = format!("org_limit:{scope}:{model}:{interval}");
        let label = if scope == "model" {
            limit["modelLabel"].as_str().unwrap_or(model)
        } else {
            "Organization"
        };
        let mut value = allowance(
            limit.get("spent").and_then(decimal),
            limit.get("limit").and_then(decimal),
            "USD",
        );
        if value.limit.is_some_and(|cap| cap <= Decimal::ZERO) {
            value.used_percent = Some(Decimal::from(100));
        }
        value.period_end_ms = timestamp(&limit["resetAt"]);
        value.reset_behavior = if value.period_end_ms.is_some() {
            QuotaResetBehavior::Periodic
        } else {
            QuotaResetBehavior::Unknown
        };
        let model_scope = if scope == "model" {
            if model.is_empty() {
                QuotaScope::Unknown
            } else {
                QuotaScope::Models(vec![model.into()])
            }
        } else {
            QuotaScope::All
        };
        let reached =
            limit["exceeded"] == true || value.limit.is_some_and(|cap| cap <= Decimal::ZERO);
        entries.push(entry(
            &id,
            "commandcode_org_limits",
            format!(
                "{label} spend limit · {interval}{}",
                if reached { " · limit reached" } else { "" }
            ),
            QuotaSubject::Organization,
            model_scope,
            QuotaValue::Budget(value),
        ));
    }

    // This is account usage even though this gateway does not run sandbox tasks.
    if payload["sandboxAccess"] == true
        && let Some(minutes) = payload.get("sandboxMinutes").filter(|v| v.is_object())
    {
        let used = minutes.get("usedMinutes").and_then(decimal);
        let limit = minutes
            .get("limitMinutes")
            .and_then(decimal)
            .filter(|limit| *limit > Decimal::ZERO);
        if used.is_some_and(|used| used > Decimal::ZERO) || limit.is_some() {
            let mut value = allowance(used, limit, "minutes");
            value.period_end_ms = timestamp(&minutes["resetAt"]);
            value.reset_behavior = QuotaResetBehavior::Periodic;
            entries.push(entry(
                "sandbox_minutes",
                "commandcode_sandbox",
                "Cloud minutes this month".into(),
                subject,
                QuotaScope::Unknown,
                QuotaValue::Window(value),
            ));
        }
    }
    Ok(entries)
}

/// Match CLI URLSearchParams: encode each value and omit unknown parameters.
fn endpoint(path: &str, org: Option<&str>, since: Option<&str>) -> String {
    let query = [("orgId", org), ("since", since)]
        .into_iter()
        .filter_map(|(name, value)| {
            value.map(|value| {
                format!(
                    "{name}={}",
                    percent_encoding::utf8_percent_encode(
                        value,
                        percent_encoding::NON_ALPHANUMERIC
                    )
                )
            })
        })
        .collect::<Vec<_>>()
        .join("&");
    if query.is_empty() {
        path.into()
    } else {
        format!("{path}?{query}")
    }
}

impl QuotaQuery for CommandCode {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let base = base_url(ctx.provider);
            let key = api_key(ctx.credential.secret)?;
            let get = async |path: &str| -> Result<Value, ChannelError> {
                let (status, _, body) = send(
                    ctx.client,
                    Method::GET,
                    &format!("{base}{path}"),
                    request::headers(key)?,
                    None,
                )
                .await?;
                serde_json::from_slice(&require_success(status, body)?)
                    .map_err(|e| ChannelError::InvalidResponse(e.to_string()))
            };
            let whoami = get("/alpha/whoami?limits=1").await?;
            let org = whoami.pointer("/org/id").and_then(Value::as_str);
            let credits_path = endpoint("/alpha/billing/credits", org, None);
            let subscription_path = endpoint("/alpha/billing/subscriptions", org, None);
            let (credits, subscription) =
                futures_util::try_join!(get(&credits_path), get(&subscription_path))?;
            let subscription = subscription
                .get("data")
                .filter(|v| v.is_null() || v.is_object())
                .ok_or_else(|| {
                    ChannelError::InvalidResponse(
                        "Command Code: invalid subscription response".into(),
                    )
                })?;
            let since = subscription["currentPeriodStart"].as_str();
            let summary = get(&endpoint("/alpha/usage/summary", org, since)).await?;
            if !summary.is_object() {
                return Err(ChannelError::InvalidResponse(
                    "Command Code: invalid usage summary".into(),
                ));
            }
            Ok(QuotaSnapshot {
                observed_at_ms: 0,
                entries: entries(
                    &whoami,
                    &credits,
                    subscription,
                    &summary,
                    if org.is_some() {
                        QuotaSubject::Organization
                    } else {
                        QuotaSubject::Account
                    },
                )?,
            })
        })
    }
}
