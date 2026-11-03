//! Account quota: `GET /api/organizations/{org}/usage` with the session
//! cookie (v3 `claudeweb/quota.rs`, `shared/claude/quota_scope.rs`). The
//! reply carries rolling `five_hour` / `seven_day` windows as
//! `{utilization, resets_at}` (percent 0-100, RFC 3339 reset), optional
//! per-family `seven_day_<family>` windows and a `limits[]` list whose
//! `weekly_scoped` entries are scoped to one model or one surface.

use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use super::{ClaudeWeb, ClaudeWebConfig, auth, prepare, read_body};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery, QuotaResetBehavior,
    QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow,
};

const FIVE_HOURS: i64 = 5 * 60 * 60;
const SEVEN_DAYS: i64 = 7 * 24 * 60 * 60;
pub const FIVE_HOUR_ID: &str = "claudeweb_five_hour";
pub const SEVEN_DAY_ID: &str = "claudeweb_seven_day";

impl QuotaModel for ClaudeWeb {
    /// Every claude.ai account has the 5-hour and 7-day windows; the
    /// per-family and surface-scoped weekly limits are only known once the
    /// usage endpoint reports them and are observed under their own ids.
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let tier = credential
            .metadata
            .get("rate_limit_tier")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if credential
                    .metadata
                    .get("pro")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    "paid".into()
                } else {
                    "free".into()
                }
            });
        let window = |id: &str, label: String, seconds: i64| QuotaDimension {
            id: id.to_owned(),
            label: Some(label),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Unit("percent".into()),
            window: QuotaWindow::Rolling { seconds },
            limit: Some(Decimal::ONE_HUNDRED),
            tracking: QuotaTracking::Reported,
            blocking: true,
        };
        vec![
            window(FIVE_HOUR_ID, format!("{tier} 5h window"), FIVE_HOURS),
            window(SEVEN_DAY_ID, format!("{tier} 7d window"), SEVEN_DAYS),
        ]
    }
}

impl QuotaQuery for ClaudeWeb {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = ClaudeWebConfig::from_view(context.provider)?;
            let auth = auth::Auth::read(&context.credential)?;
            let base = auth::base_url(context.provider);
            let url = prepare::endpoint_url(
                &config.endpoints,
                prepare::USAGE,
                &base,
                &format!("/api/organizations/{}/usage", auth.organization),
                &auth.organization,
                "",
            );
            let request =
                prepare::session_get(&url, &auth.cookie, auth.device_id.as_deref(), &base)?;
            let response = context.client.send(request).await?;
            let body = read_body(response.body).await?;
            if !response.status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status: response.status,
                    body,
                });
            }
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: parse_usage(&body)?,
            })
        })
    }
}

#[derive(Deserialize)]
struct WebUsage {
    five_hour: WebWindow,
    seven_day: WebWindow,
    #[serde(default)]
    limits: Option<Vec<WebLimit>>,
    #[serde(flatten)]
    extra: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
struct WebWindow {
    utilization: Option<f64>,
    resets_at: String,
}

#[derive(Deserialize)]
struct WebLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<WebLimitScope>,
}

#[derive(Deserialize)]
struct WebLimitScope {
    model: Option<WebLimitModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct WebLimitModel {
    display_name: Option<String>,
    id: Option<String>,
}

/// The live endpoint always carries the primary windows, so a payload
/// without them is invalid rather than "no usage".
pub(super) fn parse_usage(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let usage: WebUsage = serde_json::from_slice(body)
        .map_err(|error| ChannelError::InvalidResponse(format!("claude.ai usage: {error}")))?;
    let mut entries = vec![
        window_entry(
            FIVE_HOUR_ID.into(),
            None,
            QuotaScope::All,
            FIVE_HOURS,
            usage.five_hour.utilization,
            Some(&usage.five_hour.resets_at),
        ),
        window_entry(
            SEVEN_DAY_ID.into(),
            None,
            QuotaScope::All,
            SEVEN_DAYS,
            usage.seven_day.utilization,
            Some(&usage.seven_day.resets_at),
        ),
    ];
    let mut scoped_seen = HashSet::new();
    // One seven_day_<family> window per metered model family (opus, sonnet,
    // ...): match the naming scheme, not a fixed list.
    for (key, value) in &usage.extra {
        let Some(family_name) = key.strip_prefix("seven_day_") else {
            continue;
        };
        let Ok(window) = serde_json::from_value::<WebWindow>(value.clone()) else {
            continue;
        };
        scoped_seen.insert(format!("model:{}", slug(family_name, "scoped")));
        entries.push(window_entry(
            format!("claudeweb_{key}"),
            Some(family_name.to_owned()),
            family(family_name),
            SEVEN_DAYS,
            window.utilization,
            Some(&window.resets_at),
        ));
    }
    for limit in usage
        .limits
        .iter()
        .flatten()
        .filter(|limit| limit.kind.as_deref() == Some("weekly_scoped"))
    {
        let Some(scoped) = limit.scoped() else {
            continue;
        };
        if !scoped_seen.insert(scoped.identity()) {
            continue;
        }
        let (id, label, scope) = match scoped {
            Scoped::Model { key, label, scope } => {
                (format!("claudeweb_weekly_model_{key}"), Some(label), scope)
            }
            Scoped::Surface { key, label } => (
                format!("claudeweb_weekly_surface_{key}"),
                Some(label),
                QuotaScope::Unknown,
            ),
        };
        entries.push(window_entry(
            id,
            label,
            scope,
            SEVEN_DAYS,
            limit.percent,
            limit.resets_at.as_deref(),
        ));
    }
    Ok(entries)
}

fn window_entry(
    id: String,
    label: Option<String>,
    scope: QuotaScope,
    seconds: i64,
    utilization: Option<f64>,
    resets_at: Option<&str>,
) -> QuotaEntry {
    let used_percent = utilization.and_then(|value| Decimal::try_from(value).ok());
    let period_end_ms = resets_at
        .and_then(iso_to_unix)
        .and_then(|secs| secs.checked_mul(1000));
    QuotaEntry {
        source_id: id.clone(),
        id,
        label,
        subject: QuotaSubject::Organization,
        model_scope: scope,
        value: QuotaValue::Window(QuotaAllowance {
            used: used_percent,
            limit: Some(Decimal::ONE_HUNDRED),
            remaining: used_percent.map(|used| Decimal::ONE_HUNDRED.saturating_sub(used).max(Decimal::ZERO)),
            used_percent,
            unlimited: None,
            unit: Some("percent".into()),
            period_start_ms: period_end_ms.map(|end| end - seconds * 1000),
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}

enum Scoped {
    Model {
        key: String,
        label: String,
        scope: QuotaScope,
    },
    Surface {
        key: String,
        label: String,
    },
}

impl WebLimit {
    fn scoped(&self) -> Option<Scoped> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
            let id = non_empty(model.id.as_deref());
            let display = non_empty(model.display_name.as_deref());
            let selector = id.or(display)?;
            return Some(Scoped::Model {
                scope: model_scope(id, display),
                key: slug(selector, "scoped"),
                label: display.unwrap_or(selector).to_owned(),
            });
        }
        let surface = non_empty(scope.surface.as_deref())?;
        Some(Scoped::Surface {
            key: slug(surface, "scoped"),
            label: surface.to_owned(),
        })
    }
}

impl Scoped {
    /// A `limits[]` entry duplicating a `seven_day_<family>` window is
    /// dropped in the window's favour.
    fn identity(&self) -> String {
        match self {
            Self::Model { label, .. } => format!("model:{}", slug(label, "scoped")),
            Self::Surface { key, .. } => format!("surface:{key}"),
        }
    }
}

/// A concrete upstream model id must not expand to its whole family.
fn model_scope(id: Option<&str>, display: Option<&str>) -> QuotaScope {
    if let Some(id) = id {
        if id.starts_with("claude-") {
            return QuotaScope::Models(vec![id.to_owned()]);
        }
        return family(id);
    }
    display.map(family).unwrap_or_default()
}

/// `opus` / `Claude Opus` -> the `claude-opus` model prefix.
fn family(value: &str) -> QuotaScope {
    let value = value.trim().to_ascii_lowercase();
    let family = value.strip_prefix("claude ").unwrap_or(&value);
    if family.is_empty() || !family.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return QuotaScope::Unknown;
    }
    QuotaScope::ModelPrefixes(vec![format!("claude-{family}")])
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Lowercase, collapse every non-alphanumeric run into one `_`, trim.
fn slug(value: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let output = output.trim_matches('_');
    if output.is_empty() {
        fallback.to_owned()
    } else {
        output.to_owned()
    }
}

fn iso_to_unix(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp())
}
