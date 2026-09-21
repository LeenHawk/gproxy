//! Account quota: `GetUserStatus` over Connect's JSON codec.
//!
//! `POST /exa.seat_management_pb.SeatManagementService/GetUserStatus` with
//! `content-type: application/json` and `connect-protocol-version: 1`. The
//! session token rides in the body as `metadata.apiKey` — this endpoint takes
//! no authorization header, unlike the protobuf chat method. Two independent
//! mirrors agree on the path, the headers and the body shape:
//! `samples/windsurfapi/src/windsurf-api.js` and
//! `samples/cpa-manager-plus/apps/web/src/utils/quota/{constants,providerRequests}.ts`.
//!
//! The client identity in the body is **one mirror's, verbatim**:
//! `windsurf-api.js::buildMetadata`, which sends `ideName`/`extensionName`
//! `windsurf` with version `1.9600.41`, `locale: en`, and neither `clientName`
//! nor `os`. The two mirrors send different identities and both work, so the
//! server evidently reads only `apiKey`; that is exactly why the value here is
//! a quotation rather than a blend of the two, which is what a reader would
//! otherwise take for a capture. It is deliberately **not** the chat
//! identity from `config`: the protobuf `ClientMetadata` is a different
//! message with different field numbers, and `extensionName` has no
//! identified slot there at all (the positional JSON-to-protobuf map covers
//! `ideName` #1, `ideVersion` #2, `apiKey` #3, `locale` #4, `os` #5,
//! `extensionVersion` #7 and `clientName` #12, and stops). So the two calls
//! are consistent in the only direction the evidence allows: each sends the
//! identity that was captured on it, and neither invents a field number.
//!
//! The reply carries `userStatus.planStatus` with a daily and a weekly window
//! reported as **remaining** percent plus a unix reset second, which is a
//! `QuotaTracking::Reported` dimension each. Two parsing rules come from the
//! mirrors rather than from guesswork:
//!
//! * proto3-JSON omits zero-valued fields, so a spent window arrives as an
//!   *absent* percentage next to a present reset timestamp. Read that as 0%
//!   remaining, not as "unreported" (`windsurf-api.js::normalizeUserStatus`).
//! * percentages and unix seconds may arrive as numbers or as numeric
//!   strings, and a percentage outside 0..=100 is not a reading
//!   (`devinQuota.ts::normalizeQuotaPercent`).
//!
//! Beside the two windows the payload carries a billing ledger both mirrors
//! read: an overage balance in micro-dollars, prompt and flex credits in
//! hundredths, and the plan period as ISO-8601 instants. Those become balance
//! and budget entries here; the credit numbers are divided by 100 and the
//! overage by 1,000,000 so an operator sees dollars and credits rather than
//! the upstream's fixed-point integers.

use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::config::DevinConfig;
use super::{Devin, connect, error, request};
use crate::channel::{
    ChannelError, CredentialContext, CredentialView, OperationFuture, ProviderView, QuotaAllowance,
    QuotaBalance, QuotaDimension, QuotaEntry, QuotaMetric, QuotaModel, QuotaQuery,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
    QuotaWindow,
};

const DAY_SECONDS: i64 = 24 * 60 * 60;
const WEEK_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const DAILY_ID: &str = "devin_daily";
pub const WEEKLY_ID: &str = "devin_weekly";
/// The account's overage balance, in US dollars.
pub const OVERAGE_ID: &str = "devin_overage";
/// Prompt credits and flex credits for the plan period.
pub const PROMPT_CREDITS_ID: &str = "devin_prompt_credits";
pub const FLEX_CREDITS_ID: &str = "devin_flex_credits";

/// `windsurf-api.js::buildMetadata`, quoted. See the module documentation for
/// why this is one mirror's identity rather than the chat identity.
const QUOTA_IDE_NAME: &str = "windsurf";
const QUOTA_IDE_VERSION: &str = "1.9600.41";
const QUOTA_LOCALE: &str = "en";

impl QuotaModel for Devin {
    /// Both windows exist for every plan the mirrors describe. Whether they
    /// are rolling or calendar-aligned is not evidenced — only a reset instant
    /// is reported — so they are declared rolling, as the other
    /// percentage-window channels do.
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let plan = plan_name(credential.metadata).unwrap_or_else(|| "unknown".into());
        let window = |id: &str, label: String, seconds: i64| QuotaDimension {
            id: id.to_owned(),
            label: Some(label),
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Unit("percent".into()),
            window: QuotaWindow::Rolling { seconds },
            limit: Some(Decimal::ONE_HUNDRED),
            tracking: QuotaTracking::Reported,
        };
        vec![
            window(DAILY_ID, format!("{plan} daily window"), DAY_SECONDS),
            window(WEEKLY_ID, format!("{plan} weekly window"), WEEK_SECONDS),
        ]
    }
}

fn plan_name(metadata: &Value) -> Option<String> {
    ["plan", "plan_name", "planName"]
        .into_iter()
        .find_map(|key| metadata.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(str::to_owned)
}

impl QuotaQuery for Devin {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = DevinConfig::from_view(context.provider)?;
            let auth = request::auth(&context.credential)?;
            let url = format!(
                "{}{}",
                DevinConfig::base_url(context.provider),
                connect::USER_STATUS_PATH
            );
            let body = json!({
                "metadata": {
                    "apiKey": auth.token,
                    "ideName": QUOTA_IDE_NAME,
                    "ideVersion": QUOTA_IDE_VERSION,
                    "extensionName": QUOTA_IDE_NAME,
                    "extensionVersion": QUOTA_IDE_VERSION,
                    "locale": QUOTA_LOCALE,
                }
            });
            let mut builder = http::Request::post(&url);
            if let Some(headers) = builder.headers_mut() {
                *headers = connect::json_headers();
                config.static_headers(headers)?;
            }
            let request = builder
                .body(gproxy_protocol::HttpBody::Bytes(
                    gproxy_protocol::connection::Bytes::from(
                        serde_json::to_vec(&body)
                            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?,
                    ),
                ))
                .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
            let (status, bytes) = super::call(&*context.client, request).await?;
            if !status.is_success() {
                // Classified, not passed through: the account endpoint wraps
                // transient faults in the same 401/403 shell the chat method
                // does, and this is the call a health probe leans on hardest.
                // See `error.rs`.
                return Err(error::from_response(status, &bytes));
            }
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: parse_status(&bytes)?,
            })
        })
    }
}

/// `userStatus.planStatus` to the daily and weekly entries. A window with
/// neither a percentage nor a reset is not reported at all, which is how a
/// plan without that window reads.
pub(super) fn parse_status(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::InvalidResponse(format!("devin user status: {error}")))?;
    let status = payload
        .pointer("/userStatus/planStatus")
        .filter(|status| status.is_object())
        .ok_or_else(|| {
            ChannelError::InvalidResponse("devin user status has no planStatus".into())
        })?;
    let label = status
        .pointer("/planInfo/planName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|plan| !plan.is_empty())
        .map(str::to_owned);
    let mut entries = Vec::new();
    for (id, percent_key, reset_key, seconds) in [
        (
            DAILY_ID,
            "dailyQuotaRemainingPercent",
            "dailyQuotaResetAtUnix",
            DAY_SECONDS,
        ),
        (
            WEEKLY_ID,
            "weeklyQuotaRemainingPercent",
            "weeklyQuotaResetAtUnix",
            WEEK_SECONDS,
        ),
    ] {
        let reset = unix_seconds(status.get(reset_key));
        let Some(remaining) = percent(status.get(percent_key)).or_else(|| {
            // Absent percentage plus a present reset: proto3-JSON dropped a
            // zero, so the window is spent rather than unreported.
            reset.map(|_| Decimal::ZERO)
        }) else {
            continue;
        };
        let period_end_ms = reset.and_then(|seconds| seconds.checked_mul(1000));
        entries.push(QuotaEntry {
            id: id.to_owned(),
            source_id: id.to_owned(),
            label: label.clone(),
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(QuotaAllowance {
                used: Some(Decimal::ONE_HUNDRED - remaining),
                limit: Some(Decimal::ONE_HUNDRED),
                remaining: Some(remaining),
                used_percent: Some(Decimal::ONE_HUNDRED - remaining),
                unlimited: None,
                unit: Some("percent".into()),
                period_start_ms: period_end_ms.map(|end| end - seconds * 1000),
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
            }),
        });
    }
    entries.extend(ledger(status, label.as_ref()));
    Ok(entries)
}

/// The billing ledger beside the two windows. Both mirrors read it and
/// neither window carries it: `overageBalanceMicros` in micro-dollars
/// (`devinQuota.ts`), and prompt and flex credits in **hundredths** — used,
/// available, and the plan's monthly allowance — which is why every credit
/// number here is divided by a hundred. The plan period (`planStart` and
/// `planEnd`, ISO-8601 strings) becomes the credit entries' window, because
/// that is the period the allowance is granted over.
///
/// Anything absent is simply not reported: proto3-JSON drops zeroes, and a
/// zero balance and an unreported one are not the same fact. Unlike the
/// percentage windows there is no sibling field here that proves the account
/// has the feature at all, so the zero-means-spent rule does not apply.
fn ledger(status: &Value, label: Option<&String>) -> Vec<QuotaEntry> {
    let entry = |id: &str, value: QuotaValue| QuotaEntry {
        id: id.to_owned(),
        source_id: id.to_owned(),
        label: label.cloned(),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value,
    };
    let period_start_ms = iso_8601_ms(status.get("planStart"));
    let period_end_ms = iso_8601_ms(status.get("planEnd"));
    let mut entries = Vec::new();
    if let Some(micros) = number(status.get("overageBalanceMicros")) {
        entries.push(entry(
            OVERAGE_ID,
            QuotaValue::Balance(QuotaBalance {
                remaining: Some(micros / Decimal::from(1_000_000)),
                unit: Some("usd".into()),
            }),
        ));
    }
    for (id, used_key, available_key, monthly_key) in [
        (
            PROMPT_CREDITS_ID,
            "usedPromptCredits",
            "availablePromptCredits",
            "monthlyPromptCredits",
        ),
        (
            FLEX_CREDITS_ID,
            "usedFlexCredits",
            "availableFlexCredits",
            "monthlyFlexCreditPurchaseAmount",
        ),
    ] {
        let hundredths =
            |value: Option<&Value>| number(value).map(|value| value / Decimal::ONE_HUNDRED);
        let used = hundredths(status.get(used_key));
        let remaining = hundredths(status.get(available_key));
        let limit = hundredths(status.pointer(&format!("/planInfo/{monthly_key}")));
        if used.is_none() && remaining.is_none() && limit.is_none() {
            continue;
        }
        entries.push(entry(
            id,
            QuotaValue::Budget(QuotaAllowance {
                used,
                limit,
                remaining,
                used_percent: None,
                unlimited: None,
                unit: Some("credits".into()),
                period_start_ms,
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
            }),
        ));
    }
    entries
}

/// A number or a numeric string, which is how every scalar in this payload
/// may arrive.
fn number(value: Option<&Value>) -> Option<Decimal> {
    match value? {
        Value::Number(number) => Decimal::try_from(number.as_f64()?).ok(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// `planStart` and `planEnd` are ISO-8601 instants rather than the unix
/// seconds the two windows use.
fn iso_8601_ms(value: Option<&Value>) -> Option<i64> {
    let text = value?.as_str()?.trim();
    let stamp =
        time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()?;
    Some(stamp.unix_timestamp() * 1000)
}

/// A percentage as a number or a numeric string, rejected outside 0..=100.
fn percent(value: Option<&Value>) -> Option<Decimal> {
    let value = match value? {
        Value::Number(number) => Decimal::try_from(number.as_f64()?).ok()?,
        Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    };
    (value >= Decimal::ZERO && value <= Decimal::ONE_HUNDRED).then_some(value)
}

/// A unix second as a number or a numeric string; zero and negative values
/// are not instants.
fn unix_seconds(value: Option<&Value>) -> Option<i64> {
    let seconds = match value? {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.trim().parse().ok()?,
        _ => return None,
    };
    (seconds > 0).then_some(seconds)
}
