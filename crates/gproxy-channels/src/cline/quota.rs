use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaAllowance, QuotaAvailability, QuotaBalance, QuotaEntry, QuotaKind,
    QuotaQueryMode, QuotaSource, QuotaSubject, QuotaSupport, QuotaValue,
};
use serde_json::Value;

pub(super) fn sources(secret: &Value) -> Vec<QuotaSource> {
    let ready = super::auth::field(secret, "access_token").is_some()
        && super::auth::field(secret, "user_id").is_some();
    let plan_ready = super::auth::field(secret, "api_key").is_some()
        || super::auth::field(secret, "access_token").is_some();
    vec![QuotaSource {
        id: "balance".into(), label: "Cline account credits".into(), kinds: vec![QuotaKind::Balance],
        mode: if ready { QuotaQueryMode::Probe } else { QuotaQueryMode::Unavailable },
        support: if ready { QuotaSupport::Ready } else { QuotaSupport::Unsupported },
        reason: Some(if ready {
            "Uses the official client's internal balance endpoint with the existing login token."
        } else {
            "Balance lookup needs the existing Cline login token and user identity. Manually supplied API key support is not confirmed; sign in through Cline to obtain that identity."
        }.into()),
        automatic: ready,
    }, QuotaSource {
        id: "plan_usage".into(),
        label: "Cline plan usage limits".into(),
        kinds: vec![QuotaKind::Window],
        mode: if plan_ready { QuotaQueryMode::Probe } else { QuotaQueryMode::Unavailable },
        support: if plan_ready { QuotaSupport::Ready } else { QuotaSupport::RequiresAuthorization },
        reason: Some(if plan_ready {
            "Uses the existing API key or login token to query the Cline dashboard's internal plan endpoint. Availability depends on the account's plan."
        } else {
            "Plan usage lookup needs a Cline API key or login token."
        }.into()),
        automatic: plan_ready,
    }]
}

pub(super) fn prepare(
    source: &str,
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    if !sources(secret)
        .iter()
        .any(|item| item.id == source && item.support == QuotaSupport::Ready)
    {
        return Ok(None);
    }
    let path = if source == "plan_usage" {
        "/users/me/plan/usage-limits".into()
    } else {
        let user = super::auth::field(secret, "user_id").expect("validated by quota source");
        format!(
            "/users/{}/balance",
            crate::shared::http::encode_component(user)
        )
    };
    let uri = crate::shared::http::join(super::prepare::base_url(settings), &path, None)?;
    let mut request = http::Request::get(crate::shared::http::strip_userinfo(uri)?)
        .header("accept", "application/json")
        .body(Bytes::new())
        .map_err(|_| ChannelError::Prepare("Invalid Cline quota request".into()))?;
    if let Some(key) = super::auth::field(secret, "api_key").filter(|_| source == "plan_usage") {
        request.headers_mut().insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::Prepare("Invalid Cline quota authorization".into()))?,
        );
    } else {
        super::auth::apply(request.headers_mut(), secret)?;
    }
    Ok(Some(request))
}

pub(super) fn parse(
    source: &str,
    status: http::StatusCode,
    body: &[u8],
) -> Result<Vec<QuotaEntry>, ChannelError> {
    if !matches!(source, "balance" | "plan_usage") || !status.is_success() {
        return Err(ChannelError::Prepare(format!(
            "Cline quota query failed (HTTP {status})"
        )));
    }
    let raw: Value = serde_json::from_slice(body)
        .map_err(|_| ChannelError::Prepare("Invalid Cline quota JSON".into()))?;
    if raw.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(ChannelError::Prepare(
            "Cline quota query did not report success".into(),
        ));
    }
    let data = &raw["data"];
    if source == "plan_usage" {
        return plan_usage(data);
    }
    let remaining = crate::shared::quota_balances::amount(data, "balance")?;
    Ok(vec![crate::shared::quota_balances::entry(
        "balance",
        "balance:credits",
        QuotaSubject::Account,
        QuotaValue::Balance(QuotaBalance {
            remaining: Some(remaining),
            unit: Some("credits".into()),
            availability: QuotaAvailability::Unknown,
            components: vec![],
        }),
    )])
}

fn plan_usage(data: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let limits = data
        .get("limits")
        .and_then(Value::as_array)
        .filter(|limits| !limits.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Cline returned no plan usage limits".into()))?;
    let mut seen = std::collections::HashSet::new();
    limits
        .iter()
        .map(|limit| {
            let kind = super::auth::field(limit, "type")
                .ok_or_else(|| ChannelError::Prepare("Missing Cline plan limit type".into()))?;
            if !seen.insert(kind) {
                return Err(ChannelError::Prepare(
                    "Duplicate Cline plan limit type".into(),
                ));
            }
            let percent = limit
                .get("percentUsed")
                .filter(|value| value.is_number())
                .and_then(crate::shared::quota::decimal)
                .filter(|percent| *percent >= rust_decimal::Decimal::ZERO)
                .ok_or_else(|| {
                    ChannelError::Prepare("Invalid Cline plan usage percentage".into())
                })?;
            let end = match limit.get("resetsAt") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .and_then(crate::shared::quota::iso_to_unix)
                        .ok_or_else(|| {
                            ChannelError::Prepare("Invalid Cline plan reset time".into())
                        })?,
                ),
            };
            let mut entry = crate::shared::quota_balances::entry(
                "plan_usage",
                kind,
                QuotaSubject::Account,
                QuotaValue::Window(QuotaAllowance {
                    used_percent: Some(percent),
                    period_end: end,
                    ..Default::default()
                }),
            );
            entry.label = Some(
                match kind {
                    "five_hour" => "5-hour plan usage",
                    "weekly" => "Weekly plan usage",
                    "monthly" => "Monthly plan usage",
                    other => other,
                }
                .into(),
            );
            Ok(entry)
        })
        .collect()
}

#[cfg(test)]
mod tests;
