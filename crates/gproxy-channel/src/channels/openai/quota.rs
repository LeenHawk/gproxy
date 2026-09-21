//! Account quota: the rate limits every reply carries, and the organization's
//! daily spend (v3 `shared/quota_headers.rs`, `shared/quota_management/openai.rs`).

use super::{OpenAi, OpenAiConfig, QUOTA_DEFAULT_BASE_URL};
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
use std::collections::BTreeMap;

const RESPONSE_SOURCE: &str = "response_limits";
const REPORT_SOURCE: &str = "organization_usage";
/// The rate-limit families OpenAI reports on a reply.
const DIMENSIONS: &[&str] = &["requests", "tokens"];
const DAY_SECONDS: i64 = 24 * 60 * 60;
/// The report covers the seven complete UTC days before today.
const REPORT_DAYS: i64 = 7;
/// A page of the report is small JSON; nothing here streams.
const MAX_REPORT_BYTES: usize = 1024 * 1024;
/// A guard against a cursor loop; seven days never needs this many pages.
const MAX_REPORT_PAGES: usize = 32;

fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn unix_now_secs() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// OpenAI reports a reset as a duration from now, in Go's notation:
/// `2m59.56s`, `1h2m3s500ms`, `6ms`. The pieces accumulate, so the parse
/// walks number/unit pairs rather than matching a single shape.
fn duration_seconds(raw: &str) -> Option<i64> {
    let mut total = Decimal::ZERO;
    let mut rest = raw.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        let amount: Decimal = rest[..digits].parse().ok()?;
        if amount.is_sign_negative() {
            return None;
        }
        rest = &rest[digits..];
        // `ms` before `m`, or a millisecond would be read as a minute.
        let (unit, scale) = ["ms", "s", "m", "h", "d"]
            .into_iter()
            .zip([1, 1_000, 60_000, 3_600_000, 86_400_000])
            .find(|(unit, _)| rest.starts_with(unit))?;
        rest = &rest[unit.len()..];
        total += amount * Decimal::from(scale) / Decimal::from(1000);
    }
    total.ceil().try_into().ok()
}

impl QuotaHeaders for OpenAi {
    /// `x-ratelimit-{limit,remaining,reset}-{requests,tokens}`. A dimension
    /// reporting neither a limit nor a remainder was not reported at all;
    /// absent headers mean nothing, not zero.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let model = context.upstream_model.trim();
        let now = unix_now_secs();
        let mut entries = Vec::new();
        for dimension in DIMENSIONS {
            let decimal = |field: &str| {
                header_str(context.headers, &format!("x-ratelimit-{field}-{dimension}"))
                    .and_then(|value| value.parse::<Decimal>().ok())
            };
            let limit = decimal("limit");
            let remaining = decimal("remaining");
            if limit.is_none() && remaining.is_none() {
                continue;
            }
            let period_end_ms =
                header_str(context.headers, &format!("x-ratelimit-reset-{dimension}"))
                    .and_then(duration_seconds)
                    .map(|seconds| (now + seconds).saturating_mul(1000));
            let id = if model.is_empty() {
                format!("rate:{dimension}")
            } else {
                format!("rate:{dimension}:{model}")
            };
            entries.push(QuotaEntry {
                id,
                source_id: RESPONSE_SOURCE.into(),
                label: Some((*dimension).to_owned()),
                subject: QuotaSubject::Unknown,
                model_scope: if model.is_empty() {
                    QuotaScope::Unknown
                } else {
                    QuotaScope::Models(vec![model.to_owned()])
                },
                value: QuotaValue::RateLimit(QuotaAllowance {
                    limit,
                    remaining,
                    unit: Some((*dimension).to_owned()),
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
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
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

/// One page of the report: its buckets, and the cursor of the next page.
fn page(body: &[u8], entries: &mut Vec<QuotaEntry>) -> Result<Option<String>, ChannelError> {
    let raw: Value = serde_json::from_slice(body).map_err(|e| invalid_response(e.to_string()))?;
    let more = raw
        .get("has_more")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid_response("cost report has no pagination status"))?;
    let buckets = raw
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_response("cost report has no data"))?;
    for bucket in buckets {
        let start = bucket
            .get("start_time")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid_response("cost report bucket has no start_time"))?;
        let end = bucket
            .get("end_time")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid_response("cost report bucket has no end_time"))?;
        let results = bucket
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid_response("cost report bucket has no results"))?;
        // A day may be billed in more than one currency; each is its own
        // reading, because summing them would invent an exchange rate.
        let mut totals: BTreeMap<String, Decimal> = BTreeMap::new();
        for row in results {
            let amount = row
                .get("amount")
                .ok_or_else(|| invalid_response("cost report row has no amount"))?;
            let currency = amount
                .get("currency")
                .and_then(Value::as_str)
                .map(str::to_ascii_uppercase)
                .filter(|currency| currency.len() == 3)
                .ok_or_else(|| invalid_response("cost report row has no currency"))?;
            let value = match amount.get("value") {
                Some(Value::Number(number)) => number.to_string().parse::<Decimal>().ok(),
                Some(Value::String(text)) => text.parse::<Decimal>().ok(),
                _ => None,
            }
            .ok_or_else(|| invalid_response("cost report row has no amount value"))?;
            let total = totals.entry(currency).or_default();
            *total = total
                .checked_add(value)
                .ok_or_else(|| invalid_response("cost report amount overflow"))?;
        }
        for (currency, used) in totals {
            entries.push(QuotaEntry {
                id: format!("usage:{start}:{currency}"),
                source_id: REPORT_SOURCE.into(),
                label: None,
                subject: QuotaSubject::Organization,
                model_scope: QuotaScope::All,
                value: QuotaValue::Budget(QuotaAllowance {
                    used: Some(used),
                    unit: Some(currency),
                    period_start_ms: Some(start.saturating_mul(1000)),
                    period_end_ms: Some(end.saturating_mul(1000)),
                    ..QuotaAllowance::default()
                }),
            });
        }
    }
    if !more {
        return Ok(None);
    }
    raw.get("next_page")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
        .map(Some)
        .ok_or_else(|| invalid_response("cost report has more pages but no cursor"))
}

/// Percent-encode a query component; the cursor is opaque.
fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

impl QuotaQuery for OpenAi {
    /// `GET {base}/v1/organization/costs` over the seven complete UTC days
    /// before today, following `next_page` until the report is whole. The
    /// endpoint takes an Admin key only, which an inference key is not, so
    /// without `secret.quota_api_key` there is nothing to ask with.
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = OpenAiConfig::from_view(context.provider)?;
            let key = context
                .credential
                .secret
                .get("quota_api_key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .ok_or(ChannelError::InvalidCredential)?;
            let base = config
                .quota_base_url
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(QUOTA_DEFAULT_BASE_URL)
                .trim_end_matches('/')
                .trim_end_matches("/v1");
            let today = unix_now_secs();
            let today = today - today.rem_euclid(DAY_SECONDS);
            let url = format!(
                "{base}/v1/organization/costs\
                 ?start_time={}&end_time={today}&bucket_width=1d&limit={REPORT_DAYS}",
                today - REPORT_DAYS * DAY_SECONDS
            );
            let mut entries = Vec::new();
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_REPORT_PAGES {
                let page_url = match &cursor {
                    Some(cursor) => format!("{url}&page={}", encode_component(cursor)),
                    None => url.clone(),
                };
                let body = get(context.client, &page_url, key).await?;
                match page(&body, &mut entries)? {
                    Some(next) => cursor = Some(next),
                    None => {
                        return Ok(QuotaSnapshot {
                            // The host stamps receipt; the payload carries none.
                            observed_at_ms: 0,
                            entries,
                        });
                    }
                }
            }
            Err(invalid_response("cost report did not end within its pages"))
        })
    }
}
