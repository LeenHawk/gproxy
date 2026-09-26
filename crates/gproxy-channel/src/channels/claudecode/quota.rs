//! Account quota: the rolling 5-hour and 7-day windows of a Claude.ai plan
//! plus a 7-day window per scoped model family, read from
//! `GET {base}/api/oauth/usage` (v3 `claudecode/quota.rs`) and from the
//! `anthropic-ratelimit-unified-*` response headers the CLI displays
//! (`samples/claude-code-2.1.252`). Both paths name one window by one id:
//! the headers use claim codenames (`CLAIMS`), the usage body uses keys
//! (`five_hour`) and `limits[]` kinds (`weekly_scoped` + model family).

use super::{Claudecode, account, base_url, fact, invalid_response, send};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBreakdownRow, QuotaDimension, QuotaEntry, QuotaHeaderContext, QuotaHeaders, QuotaMetric, QuotaModel,
    QuotaQuery, QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking,
    QuotaValue, QuotaWindow,
};
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const FIVE_HOURS: i64 = 5 * 60 * 60;
const SEVEN_DAYS: i64 = 7 * 24 * 60 * 60;
/// Model families with their own weekly window. Live `/api/oauth/usage`
/// (2026-09-26) reports only Fable, and only in `limits[]`;
/// `seven_day_opus`/`seven_day_sonnet` are null and left undeclared.
const SCOPED_FAMILIES: &[&str] = &["fable"];

/// One `anthropic-ratelimit-unified-{claim}-*` header family and the window
/// id the usage body gives the same window. Claim codenames are the
/// upstream's and churn: `7d_oi` is `seven_day_overage_included`, which CLI
/// 2.1.252 labels "Fable 5 limit"; it arrives only on Fable requests and
/// moves with the Fable `weekly_scoped` limit.
struct Claim {
    codename: &'static str,
    id: &'static str,
    seconds: i64,
    family: Option<&'static str>,
}

const CLAIMS: &[Claim] = &[
    Claim {
        codename: "5h",
        id: "five_hour",
        seconds: FIVE_HOURS,
        family: None,
    },
    Claim {
        codename: "7d",
        id: "seven_day",
        seconds: SEVEN_DAYS,
        family: None,
    },
    Claim {
        codename: "7d_oi",
        id: "seven_day_fable",
        seconds: SEVEN_DAYS,
        family: Some("fable"),
    },
];

/// The window id of a family's weekly limit, the same on both paths.
fn family_window_id(family: &str) -> String {
    format!("seven_day_{family}")
}

fn plan(credential: &CredentialView<'_>) -> String {
    fact(credential, "rate_limit_tier")
        .or_else(|| fact(credential, "organization_type"))
        .unwrap_or("unknown")
        .to_owned()
}

fn dimension(id: &str, label: String, scope: QuotaScope, seconds: i64) -> QuotaDimension {
    QuotaDimension {
        id: id.to_owned(),
        label: Some(label),
        scope,
        operations: None,
        metric: QuotaMetric::Unit("percent".into()),
        window: QuotaWindow::Rolling { seconds },
        limit: Some(Decimal::ONE_HUNDRED),
        tracking: QuotaTracking::Reported,
    }
}

impl QuotaModel for Claudecode {
    /// Every plan has the account's `five_hour` and `seven_day` windows plus
    /// per-family weekly windows; other keys the endpoint may report
    /// (`seven_day_oauth_apps`, codenamed keys, surfaces, the weekly
    /// breakdown) are observe-only.
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let plan = plan(&credential);
        let mut dimensions = vec![
            dimension(
                "five_hour",
                format!("{plan} 5h window"),
                QuotaScope::All,
                FIVE_HOURS,
            ),
            dimension(
                "seven_day",
                format!("{plan} 7d window"),
                QuotaScope::All,
                SEVEN_DAYS,
            ),
        ];
        for family in SCOPED_FAMILIES {
            dimensions.push(dimension(
                &family_window_id(family),
                format!("{plan} 7d {family} window"),
                QuotaScope::ModelPrefixes(vec![format!("claude-{family}")]),
                SEVEN_DAYS,
            ));
        }
        dimensions
    }
}

fn window_entry(
    id: String,
    label: Option<String>,
    scope: QuotaScope,
    used_percent: Option<Decimal>,
    duration_secs: Option<i64>,
    period_end_ms: Option<i64>,
) -> QuotaEntry {
    let period_start_ms = match (period_end_ms, duration_secs) {
        (Some(end), Some(seconds)) => seconds.checked_mul(1000).map(|w| end - w),
        _ => None,
    };
    QuotaEntry {
        source_id: id.clone(),
        id,
        label,
        subject: QuotaSubject::Account,
        model_scope: scope,
        value: QuotaValue::Window(QuotaAllowance {
            used: used_percent,
            limit: Some(Decimal::ONE_HUNDRED),
            remaining: used_percent.map(|used| (Decimal::ONE_HUNDRED - used).max(Decimal::ZERO)),
            used_percent,
            unlimited: None,
            unit: Some("percent".into()),
            period_start_ms,
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}

/// `five_hour`/`seven_day` cover everything; `seven_day_<family>` is the
/// `claude-<family>` prefix (v3 `shared/claude/quota_scope.rs`).
fn window_scope(key: &str) -> QuotaScope {
    match key {
        "five_hour" | "seven_day" => QuotaScope::All,
        key => key
            .strip_prefix("seven_day_")
            .map(family_scope)
            .unwrap_or_default(),
    }
}

/// A concrete `claude-*` id is that model only; a family name or display
/// name such as `Claude Opus` is the family prefix.
fn model_scope(id: Option<&str>, display: Option<&str>) -> QuotaScope {
    let id = id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(id) = id {
        if id.starts_with("claude-") {
            return QuotaScope::Models(vec![id.to_owned()]);
        }
        return family_scope(id);
    }
    display.map(family_scope).unwrap_or_default()
}

fn family_scope(value: &str) -> QuotaScope {
    let value = value.trim().to_ascii_lowercase();
    let family = value.strip_prefix("claude ").unwrap_or(&value);
    if family.is_empty() || !family.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return QuotaScope::Unknown;
    }
    QuotaScope::ModelPrefixes(vec![format!("claude-{family}")])
}

fn percent(value: Option<f64>) -> Option<Decimal> {
    value.and_then(|value| Decimal::try_from(value).ok())
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1000)
}

fn present(value: &Value, field: &str) -> bool {
    value.get(field).is_some_and(|value| !value.is_null())
}

/// One rolling window: `utilization` is a percentage (0–100), `resets_at`
/// an ISO-8601 timestamp.
#[derive(Deserialize)]
struct UsageWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct UsagePayload {
    #[serde(default)]
    limits: Option<Vec<UsageLimit>>,
    #[serde(flatten)]
    windows: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct UsageLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<LimitScope>,
}

#[derive(Deserialize)]
struct LimitScope {
    model: Option<ScopeModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct ScopeModel {
    id: Option<String>,
    display_name: Option<String>,
}

impl UsageLimit {
    /// A model family's limit takes the family's window id so it meets the
    /// header claim for the same window; a concrete model or a surface keeps
    /// a key of its own.
    fn scope_key(&self) -> Option<String> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
            if let QuotaScope::ModelPrefixes(prefixes) =
                model_scope(model.id.as_deref(), model.display_name.as_deref())
                && let [prefix] = prefixes.as_slice()
                && let Some(family) = prefix.strip_prefix("claude-")
            {
                return Some(family_window_id(family));
            }
            let selector = [model.id.as_deref(), model.display_name.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .find(|value| !value.is_empty())?;
            return Some(format!("weekly_model:{}", slug(selector)));
        }
        let surface = scope.surface.as_deref().map(str::trim)?;
        (!surface.is_empty()).then(|| format!("weekly_surface:{}", slug(surface)))
    }
}

fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub(super) fn usage_entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let usage: UsagePayload =
        serde_json::from_slice(body).map_err(|e| invalid_response(e.to_string()))?;
    let mut entries = Vec::new();
    for (key, value) in &usage.windows {
        if key == "seven_day_breakdown" {
            let rows = value.get("rows").and_then(Value::as_array).into_iter().flatten()
                .filter_map(|row| Some(QuotaBreakdownRow {
                    key: row.get("key")?.as_str()?.to_owned(),
                    label: row.get("display_name").and_then(Value::as_str).map(str::to_owned),
                    percent: percent(row.get("percent").and_then(Value::as_f64))?,
                })).collect::<Vec<_>>();
            if !rows.is_empty() {
                entries.push(QuotaEntry {
                    id: key.clone(), source_id: key.clone(), label: None,
                    subject: QuotaSubject::Account, model_scope: QuotaScope::All,
                    value: QuotaValue::Breakdown(rows),
                });
            }
            continue;
        }
        let duration = match key.as_str() {
            "five_hour" => Some(FIVE_HOURS),
            key if key == "seven_day" || key.starts_with("seven_day_") => Some(SEVEN_DAYS),
            // An unknown key is a window only when both fields are real;
            // explicit nulls would otherwise mint an empty perpetual window.
            _ if present(value, "utilization") && present(value, "resets_at") => None,
            _ => continue,
        };
        let Ok(window) = serde_json::from_value::<UsageWindow>(value.clone()) else {
            continue;
        };
        entries.push(window_entry(
            key.clone(),
            None,
            window_scope(key),
            percent(window.utilization),
            duration,
            window.resets_at.as_deref().and_then(iso_to_ms),
        ));
    }
    for limit in usage.limits.iter().flatten() {
        let Some(kind) = limit.kind.as_deref() else {
            continue;
        };
        let (key, duration) = match kind {
            "session" => ("five_hour".to_owned(), FIVE_HOURS),
            "weekly_all" => ("seven_day".to_owned(), SEVEN_DAYS),
            "weekly_scoped" => match limit.scope_key() {
                Some(key) => (key, SEVEN_DAYS),
                None => continue,
            },
            _ => continue,
        };
        if entries.iter().any(|existing| existing.id == key) {
            continue;
        }
        let model = limit.scope.as_ref().and_then(|scope| scope.model.as_ref());
        let scope = match (kind, model) {
            ("weekly_scoped", Some(model)) => {
                model_scope(model.id.as_deref(), model.display_name.as_deref())
            }
            _ => window_scope(&key),
        };
        entries.push(window_entry(
            key,
            model.and_then(|model| model.display_name.clone()),
            scope,
            percent(limit.percent),
            Some(duration),
            limit.resets_at.as_deref().and_then(iso_to_ms),
        ));
    }
    Ok(entries)
}

impl QuotaQuery for Claudecode {
    /// `GET {base}/api/oauth/usage` with the CLI user agent; without it the
    /// endpoint serves an aggressively rate-limited bucket (v3 `quota.rs`).
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let account = account(&context.credential)?;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::AUTHORIZATION,
                super::header_value(&format!("Bearer {}", account.access_token))?,
            );
            headers.insert(
                header::ACCEPT,
                HeaderValue::from_static("application/json, text/plain, */*"),
            );
            headers.insert(
                header::USER_AGENT,
                HeaderValue::from_static(super::CLI_USER_AGENT),
            );
            headers.insert(
                "anthropic-beta",
                HeaderValue::from_static(super::OAUTH_BETA),
            );
            let (status, _, bytes) = send(
                context.client,
                Method::GET,
                &format!("{}/api/oauth/usage", base_url(context.provider)),
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
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: usage_entries(&bytes)?,
            })
        })
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

impl QuotaHeaders for Claudecode {
    /// `anthropic-ratelimit-unified-{claim}-utilization` is a 0..1 fraction
    /// the CLI clamps and shows as a percentage; `-reset` is Unix seconds
    /// (`samples/claude-code-2.1.252`, 2.1.225+ usage UI). Absent headers
    /// mean nothing was reported; unknown claims are not read.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let headers = context.headers;
        let mut entries = Vec::new();
        for claim in CLAIMS {
            let codename = claim.codename;
            let Some(utilization) = header_str(
                headers,
                &format!("anthropic-ratelimit-unified-{codename}-utilization"),
            )
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite()) else {
                continue;
            };
            let used = percent(Some(utilization.clamp(0.0, 1.0) * 100.0));
            let reset = header_str(
                headers,
                &format!("anthropic-ratelimit-unified-{codename}-reset"),
            )
            .and_then(|value| value.parse::<f64>().ok())
            .map(|secs| (secs.round() as i64).saturating_mul(1000));
            entries.push(window_entry(
                claim.id.to_owned(),
                None,
                claim.family.map_or(QuotaScope::All, family_scope),
                used,
                Some(claim.seconds),
                reset,
            ));
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codenames churn upstream; each claim must land on a declared window
    /// with the scope the usage body gives the same window.
    #[test]
    fn header_claims_name_declared_windows() {
        let families: Vec<String> = SCOPED_FAMILIES
            .iter()
            .map(|family| family_window_id(family))
            .collect();
        for claim in CLAIMS {
            assert_eq!(window_scope(claim.id), claim.family.map_or(QuotaScope::All, family_scope));
            match claim.family {
                Some(family) => assert!(families.contains(&family_window_id(family))),
                None => assert!(matches!(claim.id, "five_hour" | "seven_day")),
            }
        }
        assert_eq!(
            CLAIMS.iter().find(|claim| claim.codename == "7d_oi").map(|claim| claim.id),
            Some("seven_day_fable")
        );
    }
}
