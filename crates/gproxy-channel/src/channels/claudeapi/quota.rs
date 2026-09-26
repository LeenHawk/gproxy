//! Account quota: the rate limits every reply carries, and the organization's
//! daily spend (v3 `shared/quota_headers.rs`, `shared/quota_claude_report.rs`).

use super::request::{api_key, base_url};
use super::{ANTHROPIC_VERSION, Claudeapi, ClaudeapiConfig, QUOTA_DEFAULT_BASE_URL};
use crate::OutboundClient;
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry,
    QuotaHeaderContext, QuotaHeaders, QuotaQuery, QuotaScope, QuotaSnapshot, QuotaSubject,
    QuotaValue,
};
use futures_util::StreamExt;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{HttpBody, WireResponse};
use http::{HeaderMap, HeaderValue, Method, header};
use rust_decimal::Decimal;
use serde_json::Value;

/// The source id both quota paths of this channel report under, so a reading
/// from a reply and one from the report stay apart.
const RESPONSE_SOURCE: &str = "response_limits";
const REPORT_SOURCE: &str = "organization_usage";
/// The rate-limit families Anthropic reports on a reply.
const DIMENSIONS: &[&str] = &["requests", "tokens", "input-tokens", "output-tokens"];
const DAY_SECONDS: i64 = 24 * 60 * 60;
/// The report covers the seven complete UTC days before today.
const REPORT_DAYS: i64 = 7;
/// The report is one small JSON page; nothing here streams.
const MAX_REPORT_BYTES: usize = 1024 * 1024;

fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1000)
}

fn unix_now_secs() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// `YYYY-MM-DDT00:00:00Z` for a UTC day boundary.
fn iso_day(unix_secs: i64) -> Option<String> {
    let stamp = time::OffsetDateTime::from_unix_timestamp(unix_secs).ok()?;
    Some(format!(
        "{:04}-{:02}-{:02}T00:00:00Z",
        stamp.year(),
        u8::from(stamp.month()),
        stamp.day()
    ))
}

impl QuotaHeaders for Claudeapi {
    /// `anthropic-ratelimit-{dimension}-{limit,remaining,reset}` for the four
    /// families Anthropic meters. `reset` is an RFC 3339 instant, not a
    /// duration. A dimension reporting neither a limit nor a remainder was
    /// not reported at all; absent headers mean nothing, not zero.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let model = context.upstream_model.trim();
        let mut entries = Vec::new();
        for dimension in DIMENSIONS {
            let decimal = |field: &str| {
                header_str(
                    context.headers,
                    &format!("anthropic-ratelimit-{dimension}-{field}"),
                )
                .and_then(|value| value.parse::<Decimal>().ok())
            };
            let limit = decimal("limit");
            let remaining = decimal("remaining");
            if limit.is_none() && remaining.is_none() {
                continue;
            }
            let period_end_ms = header_str(
                context.headers,
                &format!("anthropic-ratelimit-{dimension}-reset"),
            )
            .and_then(iso_to_ms);
            let id = if model.is_empty() {
                format!("rate:{dimension}")
            } else {
                format!("rate:{dimension}:{model}")
            };
            entries.push(QuotaEntry {
                id,
                source_id: RESPONSE_SOURCE.into(),
                label: Some(dimension.replace('-', " ")),
                subject: QuotaSubject::Unknown,
                model_scope: if model.is_empty() {
                    QuotaScope::Unknown
                } else {
                    QuotaScope::Models(vec![model.to_owned()])
                },
                value: QuotaValue::RateLimit(QuotaAllowance {
                    limit,
                    remaining,
                    unit: Some(if *dimension == "requests" {
                        "requests".into()
                    } else {
                        "tokens".into()
                    }),
                    period_end_ms,
                    ..QuotaAllowance::default()
                }),
            });
        }
        Ok(entries)
    }
}

async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| invalid_response(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_REPORT_BYTES {
                    return Err(invalid_response("cost report exceeds the read limit"));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

async fn get(client: &dyn OutboundClient, url: &str, key: &str) -> Result<Bytes, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::HeaderName::from_static("x-api-key"),
        HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(
        http::HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    let mut builder = http::Request::builder().method(Method::GET).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    let request = builder
        .body(HttpBody::Bytes(Bytes::new()))
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    let WireResponse {
        status,
        headers: _,
        body,
    } = client.send(request).await?;
    let body = read_body(body).await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse { status, body });
    }
    Ok(body)
}

/// One day of the report. The report is refused rather than under-reported
/// when Anthropic says it has more pages: this endpoint takes no cursor, so a
/// truncated report would silently understate the spend.
fn report_entries(body: &[u8]) -> Result<Vec<QuotaEntry>, ChannelError> {
    let raw: Value = serde_json::from_slice(body).map_err(|e| invalid_response(e.to_string()))?;
    if raw.get("has_more").and_then(Value::as_bool) != Some(false) {
        return Err(invalid_response(
            "cost report is incomplete or has no pagination status",
        ));
    }
    let buckets = raw
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("cost report has no data"))?;
    let mut entries = Vec::new();
    for bucket in buckets {
        let start = bucket
            .get("starting_at")
            .and_then(Value::as_str)
            .and_then(iso_to_ms)
            .ok_or_else(|| invalid_response("cost report bucket has no starting_at"))?;
        let end = bucket
            .get("ending_at")
            .and_then(Value::as_str)
            .and_then(iso_to_ms)
            .ok_or_else(|| invalid_response("cost report bucket has no ending_at"))?;
        let results = bucket
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_response("cost report bucket has no results"))?;
        let mut cents = Decimal::ZERO;
        for row in results {
            if row.get("currency").and_then(Value::as_str) != Some("USD") {
                return Err(invalid_response("cost report is not in USD"));
            }
            let amount = match row.get("amount") {
                Some(Value::Number(number)) => number.to_string().parse::<Decimal>().ok(),
                Some(Value::String(text)) => text.parse::<Decimal>().ok(),
                _ => None,
            }
            .ok_or_else(|| invalid_response("cost report row has no amount"))?;
            cents = cents
                .checked_add(amount)
                .ok_or_else(|| invalid_response("cost report amount overflow"))?;
        }
        entries.push(QuotaEntry {
            id: format!("usage:{}", start / 1000),
            source_id: REPORT_SOURCE.into(),
            label: None,
            subject: QuotaSubject::Organization,
            model_scope: QuotaScope::All,
            value: QuotaValue::Budget(QuotaAllowance {
                // v3 read `amount` as minor units; the report is in cents.
                used: Some(cents / Decimal::ONE_HUNDRED),
                unit: Some("USD".into()),
                period_start_ms: Some(start),
                period_end_ms: Some(end),
                ..QuotaAllowance::default()
            }),
        });
    }
    Ok(entries)
}

impl QuotaQuery for Claudeapi {
    /// `GET {base}/v1/organizations/cost_report` over the seven complete UTC
    /// days before today. The endpoint wants an Admin key, which the
    /// inference key usually is not, so `secret.quota_api_key` takes
    /// precedence; only when there is none does the provider's own
    /// `base_url` stand in for the report origin, because a relay proxying
    /// inference need not proxy the Admin API.
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = ClaudeapiConfig::from_view(context.provider)?;
            let admin = api_key(context.credential.secret, "quota_api_key");
            let key = admin
                .or_else(|| api_key(context.credential.secret, "api_key"))
                .ok_or(ChannelError::InvalidCredential)?;
            let base = config
                .quota_base_url
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .map(str::to_owned)
                .or_else(|| admin.is_none().then(|| base_url(context.provider)))
                .unwrap_or_else(|| QUOTA_DEFAULT_BASE_URL.to_owned());
            // A configured origin may already end in the version segment the
            // report path carries; it is written once, not twice.
            let base = base.trim_end_matches('/').trim_end_matches("/v1");
            let today = unix_now_secs() - unix_now_secs().rem_euclid(DAY_SECONDS);
            let (from, to) = (
                iso_day(today - REPORT_DAYS * DAY_SECONDS)
                    .ok_or_else(|| invalid_response("host clock is out of range"))?,
                iso_day(today).ok_or_else(|| invalid_response("host clock is out of range"))?,
            );
            let url = format!(
                "{base}/v1/organizations/cost_report\
                 ?starting_at={from}&ending_at={to}&bucket_width=1d&limit={REPORT_DAYS}"
            );
            let body = get(context.client, &url, key).await?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: report_entries(&body)?,
            })
        })
    }
}
