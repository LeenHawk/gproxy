//! Account quota windows, headers and the WHAM usage probe.

use super::common::{invalid_response, send_json};
use super::headers::{account, backend_headers, base_urls, plan_type};
use super::{Codex, CodexConfig};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaHeaderContext, QuotaHeaders, QuotaMetric,
    QuotaModel, QuotaQuery, QuotaReset, QuotaResetBehavior, QuotaResetCredits, QuotaResetOutcome,
    QuotaResetRequest, QuotaResetResult, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking,
    QuotaValue, QuotaWindow,
};
use http::{HeaderMap, Method};
use rust_decimal::Decimal;
use serde::Deserialize;

const PRIMARY_WINDOW_SECS: i64 = 5 * 60 * 60;
const SECONDARY_WINDOW_SECS: i64 = 7 * 24 * 60 * 60;

fn window_dimension(id: &str, label: &str, seconds: i64) -> QuotaDimension {
    QuotaDimension {
        id: id.to_owned(),
        label: Some(label.to_owned()),
        scope: QuotaScope::All,
        operations: None,
        metric: QuotaMetric::Unit("percent".into()),
        window: QuotaWindow::Rolling { seconds },
        limit: Some(Decimal::ONE_HUNDRED),
        tracking: QuotaTracking::Reported,
    }
}

impl QuotaModel for Codex {
    /// Every ChatGPT plan has the account's 5-hour and 7-day windows; feature
    /// limits (`x-<feature>-*`, `additional_rate_limits`) are observed as
    /// cycles under their own ids without a declared dimension.
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let plan = plan_type(&credential).unwrap_or_else(|| "unknown".into());
        vec![
            window_dimension(
                "codex_primary",
                &format!("{plan} 5h window"),
                PRIMARY_WINDOW_SECS,
            ),
            window_dimension(
                "codex_secondary",
                &format!("{plan} 7d window"),
                SECONDARY_WINDOW_SECS,
            ),
        ]
    }
}

fn percent_entry(
    id: String,
    label: Option<String>,
    used_percent: Decimal,
    window_seconds: Option<i64>,
    reset_at_secs: Option<i64>,
) -> QuotaEntry {
    let period_end_ms = reset_at_secs.and_then(|s| s.checked_mul(1000));
    let period_start_ms = match (period_end_ms, window_seconds) {
        (Some(end), Some(seconds)) => seconds.checked_mul(1000).map(|w| end - w),
        _ => None,
    };
    QuotaEntry {
        source_id: id.clone(),
        id,
        label,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            used: Some(used_percent),
            limit: Some(Decimal::ONE_HUNDRED),
            remaining: Some((Decimal::ONE_HUNDRED - used_percent).max(Decimal::ZERO)),
            used_percent: Some(used_percent),
            unlimited: None,
            unit: Some("percent".into()),
            period_start_ms,
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}

fn credits_entry(has_credits: bool, unlimited: bool, balance: Option<&str>) -> QuotaEntry {
    QuotaEntry {
        id: "codex_credits".into(),
        source_id: "codex_credits".into(),
        label: Some("credits".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            remaining: if unlimited {
                None
            } else if has_credits {
                balance.and_then(|b| b.trim().parse().ok())
            } else {
                Some(Decimal::ZERO)
            },
            unit: Some("credits".into()),
        }),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn header_decimal(headers: &HeaderMap, name: &str) -> Option<Decimal> {
    header_str(headers, name)?.parse().ok()
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    header_str(headers, name)?.parse().ok()
}

fn header_bool(headers: &HeaderMap, name: &str) -> Option<bool> {
    match header_str(headers, name)? {
        v if v.eq_ignore_ascii_case("true") || v == "1" => Some(true),
        v if v.eq_ignore_ascii_case("false") || v == "0" => Some(false),
        _ => None,
    }
}

impl QuotaHeaders for Codex {
    /// `x-<limit>-{primary,secondary}-{used-percent,window-minutes,reset-at}`
    /// for every limit family present (`codex` is the account, other names
    /// are metered features), plus `x-codex-credits-*`.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let headers = context.headers;
        let mut families: Vec<String> = headers
            .keys()
            .filter_map(|name| {
                name.as_str()
                    .strip_suffix("-primary-used-percent")?
                    .strip_prefix("x-")
                    .map(str::to_owned)
            })
            .collect();
        families.sort();
        families.dedup();
        let mut entries = Vec::new();
        for family in families {
            let limit_id = family.replace('-', "_");
            let label = header_str(headers, &format!("x-{family}-limit-name"))
                .filter(|l| !l.is_empty())
                .map(str::to_owned);
            for window in ["primary", "secondary"] {
                let Some(used) =
                    header_decimal(headers, &format!("x-{family}-{window}-used-percent"))
                else {
                    continue;
                };
                let minutes = header_i64(headers, &format!("x-{family}-{window}-window-minutes"));
                let reset = header_i64(headers, &format!("x-{family}-{window}-reset-at"));
                entries.push(percent_entry(
                    format!("{limit_id}_{window}"),
                    label.clone(),
                    used,
                    minutes.map(|m| m * 60),
                    reset,
                ));
            }
        }
        if let (Some(has), Some(unlimited)) = (
            header_bool(headers, "x-codex-credits-has-credits"),
            header_bool(headers, "x-codex-credits-unlimited"),
        ) {
            entries.push(credits_entry(
                has,
                unlimited,
                header_str(headers, "x-codex-credits-balance"),
            ));
        }
        Ok(entries)
    }
}

#[derive(Deserialize, Default)]
struct UsagePayload {
    #[serde(default)]
    rate_limit: Option<RateLimitDetails>,
    #[serde(default)]
    credits: Option<CreditDetails>,
    #[serde(default)]
    additional_rate_limits: Option<Vec<AdditionalRateLimit>>,
}
#[derive(Deserialize, Default)]
struct RateLimitDetails {
    #[serde(default)]
    primary_window: Option<WindowSnapshot>,
    #[serde(default)]
    secondary_window: Option<WindowSnapshot>,
}
#[derive(Deserialize)]
struct WindowSnapshot {
    used_percent: Decimal,
    #[serde(default)]
    limit_window_seconds: Option<i64>,
    #[serde(default)]
    reset_at: Option<i64>,
}
#[derive(Deserialize)]
struct AdditionalRateLimit {
    limit_name: String,
    metered_feature: String,
    #[serde(default)]
    rate_limit: Option<RateLimitDetails>,
}
#[derive(Deserialize)]
struct CreditDetails {
    has_credits: bool,
    unlimited: bool,
    #[serde(default)]
    balance: Option<String>,
}

fn usage_entries(payload: &UsagePayload) -> Vec<QuotaEntry> {
    let mut entries = Vec::new();
    let mut push = |id: &str, label: Option<&str>, details: &RateLimitDetails| {
        for (window, snapshot) in [
            ("primary", &details.primary_window),
            ("secondary", &details.secondary_window),
        ] {
            if let Some(snapshot) = snapshot {
                entries.push(percent_entry(
                    format!("{id}_{window}"),
                    label.map(str::to_owned),
                    snapshot.used_percent,
                    snapshot.limit_window_seconds,
                    snapshot.reset_at,
                ));
            }
        }
    };
    if let Some(details) = &payload.rate_limit {
        push("codex", None, details);
    }
    for extra in payload.additional_rate_limits.iter().flatten() {
        if let Some(details) = &extra.rate_limit {
            push(
                &extra.metered_feature.to_ascii_lowercase().replace('-', "_"),
                Some(&extra.limit_name),
                details,
            );
        }
    }
    if let Some(credits) = &payload.credits {
        entries.push(credits_entry(
            credits.has_credits,
            credits.unlimited,
            credits.balance.as_deref(),
        ));
    }
    entries
}

impl QuotaQuery for Codex {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let account = account(&context.credential)?;
            let (_, backend) = base_urls(context.provider);
            let headers = backend_headers(&config, &account, None)?;
            let (status, _, bytes) = send_json(
                context.client,
                Method::GET,
                &format!("{backend}/wham/usage"),
                headers,
                None,
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let payload: UsagePayload =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: usage_entries(&payload),
            })
        })
    }
}

#[derive(Deserialize)]
struct ResetCreditsDetails {
    available_count: u64,
    #[serde(default)]
    credits: Vec<ResetCredit>,
}

#[derive(Deserialize)]
struct ResetCredit {
    status: Option<String>,
    expires_at: Option<String>,
}

#[derive(Deserialize)]
struct ResetResponse {
    code: ResetCode,
    windows_reset: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResetCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

impl QuotaReset for Codex {
    fn credits<'a>(
        &'a self,
        context: CredentialContext<'a>,
    ) -> OperationFuture<'a, QuotaResetCredits> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let account = account(&context.credential)?;
            let (_, backend) = base_urls(context.provider);
            let (status, _, bytes) = send_json(
                context.client,
                Method::GET,
                &format!("{backend}/wham/rate-limit-reset-credits"),
                backend_headers(&config, &account, None)?,
                None,
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let details: ResetCreditsDetails =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            let mut credit_expirations_ms: Vec<_> = details
                .credits
                .iter()
                .filter(|credit| credit.status.as_deref() == Some("available"))
                .map(|credit| {
                    credit
                        .expires_at
                        .as_deref()
                        .and_then(|date| {
                            time::OffsetDateTime::parse(
                                date,
                                &time::format_description::well_known::Rfc3339,
                            )
                            .ok()
                        })
                        .map(|date| (date.unix_timestamp_nanos() / 1_000_000) as i64)
                })
                .collect();
            credit_expirations_ms.sort_by_key(|expiry| expiry.unwrap_or(i64::MAX));
            let expires_at_ms = credit_expirations_ms.iter().flatten().next().copied();
            Ok(QuotaResetCredits {
                credit_expirations_ms,
                available_count: Some(details.available_count),
                expires_at_ms,
                options: Vec::new(),
            })
        })
    }

    fn reset<'a>(
        &'a self,
        context: CredentialContext<'a>,
        request: QuotaResetRequest<'a>,
    ) -> OperationFuture<'a, QuotaResetResult> {
        Box::pin(async move {
            if request.program.is_some() || request.grant_id.is_some() {
                return Err(ChannelError::InvalidConfig(
                    "Codex reset does not accept a program or grant selection".into(),
                ));
            }
            let config = CodexConfig::from_view(context.provider)?;
            let account = account(&context.credential)?;
            let (_, backend) = base_urls(context.provider);
            let mut headers = backend_headers(&config, &account, None)?;
            headers.insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            );
            let body = serde_json::to_vec(
                &serde_json::json!({"redeem_request_id": request.redeem_request_id}),
            )
            .map_err(|e| invalid_response(e.to_string()))?;
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &format!("{backend}/wham/rate-limit-reset-credits/consume"),
                headers,
                Some(body),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let response: ResetResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            Ok(QuotaResetResult {
                reason: None,
                outcome: match response.code {
                    ResetCode::Reset => QuotaResetOutcome::Reset,
                    ResetCode::NothingToReset => QuotaResetOutcome::NothingToReset,
                    ResetCode::NoCredit => QuotaResetOutcome::NoCredit,
                    ResetCode::AlreadyRedeemed => QuotaResetOutcome::AlreadyRedeemed,
                },
                windows_reset: response.windows_reset,
            })
        })
    }
}
