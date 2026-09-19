//! Account quota: the rolling 5-hour and 7-day windows of a Claude.ai plan,
//! read from `GET {base}/api/oauth/usage` (v3 `claudecode/quota.rs`) and
//! from the `anthropic-ratelimit-unified-*` response headers the CLI
//! displays (`samples/claude-code-2.1.252`). Weekly windows scoped to a
//! model family (`seven_day_opus`, `limits[].kind == weekly_scoped`) match
//! by `claude-<family>` prefix.

use super::{Claudecode, ClaudecodeConfig, account, base_url, fact, invalid_response, send};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaDimension, QuotaEntry, QuotaHeaderContext, QuotaHeaders, QuotaMetric, QuotaModel,
    QuotaQuery, QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking,
    QuotaValue, QuotaWindow,
};
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const FIVE_HOURS: i64 = 5 * 60 * 60;
const SEVEN_DAYS: i64 = 7 * 24 * 60 * 60;
/// Model families the usage endpoint reports weekly windows for
/// (`seven_day_opus`, `seven_day_sonnet` in the 2.1.252 route inventory).
const SCOPED_FAMILIES: &[&str] = &["opus", "sonnet"];

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
    /// (`seven_day_oauth_apps`, surfaces) are observed without a dimension.
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
                &format!("seven_day_{family}"),
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
    fn scope_key(&self) -> Option<String> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
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
            let config = ClaudecodeConfig::from_view(context.provider)?;
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
                super::header_value(
                    config
                        .user_agent
                        .as_deref()
                        .unwrap_or(super::CLI_USER_AGENT),
                )?,
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
    /// `anthropic-ratelimit-unified-{5h,7d}-utilization` is a 0..1 fraction
    /// the CLI clamps and shows as a percentage; `-reset` is Unix seconds
    /// (`samples/claude-code-2.1.252`, 2.1.225+ usage UI). Absent headers
    /// mean nothing was reported.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let headers = context.headers;
        let mut entries = Vec::new();
        for (suffix, id, seconds) in [
            ("5h", "five_hour", FIVE_HOURS),
            ("7d", "seven_day", SEVEN_DAYS),
        ] {
            let Some(utilization) = header_str(
                headers,
                &format!("anthropic-ratelimit-unified-{suffix}-utilization"),
            )
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite()) else {
                continue;
            };
            let used = percent(Some(utilization.clamp(0.0, 1.0) * 100.0));
            let reset = header_str(
                headers,
                &format!("anthropic-ratelimit-unified-{suffix}-reset"),
            )
            .and_then(|value| value.parse::<f64>().ok())
            .map(|secs| (secs.round() as i64).saturating_mul(1000));
            entries.push(window_entry(
                id.to_owned(),
                None,
                QuotaScope::All,
                used,
                Some(seconds),
                reset,
            ));
        }
        Ok(entries)
    }
}
