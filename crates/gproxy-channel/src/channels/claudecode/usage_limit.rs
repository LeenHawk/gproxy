//! Claude Code 2.1.302 usage-limit requests and capacity waiting.
//! Sources: readable/chunk-pmgddf3s.js:8637-9064 (slow state/retries),
//! chunk-m1s9ka5b.js:27-91 (offer acceptance), and
//! chunk-8242685m.js:136075-136093,139066-139067 (extended dispatch).
//! All paths are under samples/claude-code-2.1.302/.

use super::{ClaudecodeConfig, unix_now_ms};
use crate::channels::shared::timer::sleep;
use crate::{
    OutboundClient,
    channel::{ChannelError, ChannelState, CredentialView},
};
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CasResult, StateWrite, Version},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, SystemTime};
#[cfg(not(target_arch = "wasm32"))]
use tokio::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

pub(super) const HEADER: &str = "anthropic-usage-limit";
const SLOW_STATUS: &str = "anthropic-ratelimit-unified-slow-status";
const FIVE_HOUR_RESET: &str = "anthropic-ratelimit-unified-5h-reset";
const SEVEN_DAY_RESET: &str = "anthropic-ratelimit-unified-7d-reset";
const OVERAGE_IN_USE: &str = "anthropic-ratelimit-unified-overage-in-use";
const REPRESENTATIVE: &str = "anthropic-ratelimit-unified-representative-claim";
const OVERAGE_STATUS: &str = "anthropic-ratelimit-unified-overage-status";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Slow,
    Extended,
}

impl Mode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "slow" => Some(Self::Slow),
            "extended" => Some(Self::Extended),
            _ => None,
        }
    }
}

pub(super) fn requested_mode(headers: &HeaderMap, config: &ClaudecodeConfig) -> Option<Mode> {
    config
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(HEADER))
        .and_then(|(_, value)| Mode::parse(value))
        .or_else(|| text(headers, HEADER).and_then(Mode::parse))
}

pub(super) fn set_mode(headers: &mut HeaderMap, mode: Option<Mode>) {
    headers.remove(HEADER);
    if let Some(mode) = mode {
        headers.insert(
            HEADER,
            HeaderValue::from_static(match mode {
                Mode::Slow => "slow",
                Mode::Extended => "extended",
            }),
        );
    }
}

pub(super) fn paid_usage_enabled(credential: &CredentialView<'_>) -> bool {
    credential
        .metadata
        .get("has_extra_usage_enabled")
        .and_then(Value::as_bool)
        .or_else(|| {
            credential
                .secret
                .pointer("/provider_fields/has_extra_usage_enabled")
                .and_then(Value::as_bool)
        })
        .or_else(|| {
            credential
                .secret
                .get("has_extra_usage_enabled")
                .and_then(Value::as_bool)
        })
        == Some(true)
}

/// A tool-result round is the continuation signal available in every adapted
/// Messages request. Private CLI query-depth/feature flags are not wire fields;
/// a native client's explicit mode is authoritative instead of guessing them.
pub(super) fn is_continuation(body: &Value) -> bool {
    body.get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.last())
        .is_some_and(|message| {
            message.get("role").and_then(Value::as_str) == Some("user")
                && message
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|blocks| {
                        blocks.iter().any(|block| {
                            block.get("type").and_then(Value::as_str) == Some("tool_result")
                        })
                    })
        })
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

fn number(headers: &HeaderMap, name: &str) -> Option<f64> {
    let value = text(headers, name)?.trim().parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

fn reset_ms(headers: &HeaderMap, name: &str) -> Option<i64> {
    number(headers, name).map(|seconds| (seconds * 1000.0).round() as i64)
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
struct Session {
    slow_reset_ms: Option<i64>,
    budget_until_ms: Option<i64>,
    cooloff_until_ms: Option<i64>,
    extra_usage_enabled: Option<bool>,
    retry_after_ms: Option<u64>,
    max_wait_ms: Option<u64>,
}

impl Session {
    fn active(&mut self, now: i64) -> bool {
        if self
            .slow_reset_ms
            .is_some_and(|reset| now >= reset.saturating_add(60_000))
        {
            self.slow_reset_ms = None;
        }
        self.slow_reset_ms.is_some()
    }

    fn accept_offer(&mut self, response: &WireResponse<HttpBody>, now: i64) -> bool {
        let headers = &response.headers;
        if response.status != StatusCode::TOO_MANY_REQUESTS
            || text(headers, "anthropic-ratelimit-unified-slow-offer") != Some("treatment")
            || text(headers, REPRESENTATIVE) != Some("five_hour")
            || self.budget_until_ms.is_some_and(|until| now < until)
            || self.cooloff_until_ms.is_some_and(|until| now < until)
            || text(headers, OVERAGE_IN_USE) == Some("true")
            || self.active(now)
        {
            return false;
        }
        let reset = reset_ms(headers, "anthropic-ratelimit-unified-reset")
            .or_else(|| reset_ms(headers, FIVE_HOUR_RESET));
        if let Some(reset) = reset.filter(|reset| *reset > now) {
            self.slow_reset_ms = Some(reset);
            return true;
        }
        false
    }

    fn observe(
        &mut self,
        response: &WireResponse<HttpBody>,
        mode: Option<Mode>,
        now: i64,
        main: bool,
    ) -> bool {
        let headers = &response.headers;
        if let Some(reason) = text(
            headers,
            "anthropic-ratelimit-unified-overage-disabled-reason",
        ) {
            if !matches!(reason, "" | "unknown" | "fetch_error") {
                self.extra_usage_enabled = Some(false);
            }
        } else if text(headers, OVERAGE_IN_USE) == Some("true")
            || matches!(
                text(headers, OVERAGE_STATUS),
                Some("allowed" | "allowed_warning")
            )
        {
            self.extra_usage_enabled = Some(true);
        }
        if mode != Some(Mode::Slow) {
            return false;
        }
        let status = text(headers, SLOW_STATUS);
        if response.status.is_success() {
            if let Some(reset) = reset_ms(headers, FIVE_HOUR_RESET) {
                if self
                    .slow_reset_ms
                    .is_some_and(|previous| reset >= previous.saturating_add(60_000))
                {
                    self.slow_reset_ms = None;
                } else if matches!(status, Some("active" | "not_needed")) {
                    self.slow_reset_ms = Some(reset);
                }
            }
            if status == Some("ineligible") && text(headers, OVERAGE_IN_USE) == Some("true") {
                self.slow_reset_ms = None;
            }
        } else if response.status == StatusCode::TOO_MANY_REQUESTS {
            let stop = match status {
                Some("weekly_limit" | "budget_exhausted" | "off") => true,
                Some("ineligible") => main,
                Some("slot_busy") => false,
                _ => {
                    main && (headers.contains_key(REPRESENTATIVE)
                        || headers.contains_key(OVERAGE_STATUS))
                }
            };
            if stop {
                self.slow_reset_ms = None;
            }
            if status == Some("budget_exhausted") {
                self.budget_until_ms =
                    reset_ms(headers, "anthropic-ratelimit-unified-slow-budget-reset")
                        .filter(|reset| *reset > 0)
                        .or_else(|| reset_ms(headers, SEVEN_DAY_RESET))
                        .map(|reset| reset.min(now.saturating_add(8 * 24 * 60 * 60 * 1000)));
            }
            return stop;
        }
        false
    }
}

struct Wait {
    since: Option<Instant>,
    retry: Duration,
    maximum: Duration,
}

impl Default for Wait {
    fn default() -> Self {
        Self {
            since: None,
            retry: Duration::from_secs(20),
            maximum: Duration::from_secs(1200),
        }
    }
}

impl Wait {
    fn observe(&mut self, headers: &HeaderMap) {
        if let Some(seconds) = number(headers, "anthropic-ratelimit-unified-slow-retry-after") {
            self.retry =
                Duration::from_millis((seconds * 1000.0).round().clamp(5000.0, 600_000.0) as u64);
        }
        if let Some(seconds) = number(headers, "anthropic-ratelimit-unified-slow-max-wait") {
            self.maximum = Duration::from_millis(
                (seconds * 1000.0).round().clamp(60_000.0, 21_600_000.0) as u64,
            );
        }
    }

    fn delay(&mut self, remaining: Duration) -> Option<Duration> {
        let since = self.since.get_or_insert_with(Instant::now);
        let left = self.maximum.saturating_sub(since.elapsed()).min(remaining);
        if left.is_zero() {
            return None;
        }
        let random = getrandom::u32().unwrap_or(u32::MAX / 2) as f64 / u32::MAX as f64;
        Some(
            Duration::from_millis(
                (self.retry.as_millis() as f64 * (0.7 + random * 0.6)).round() as u64,
            )
            .min(left),
        )
    }
}

fn overloaded_json(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .is_some_and(|body| {
            body.pointer("/error/type")
                .or_else(|| body.get("type"))
                .and_then(Value::as_str)
                == Some("overloaded_error")
        })
}

/// Inspect only a bounded unsuccessful response. Preserve every consumed byte
/// and transfer error if it is returned instead of retried; never read a 200 SSE.
async fn overloaded(response: &mut WireResponse<HttpBody>) -> bool {
    if response.status.as_u16() == 529 {
        return true;
    }
    if response.status.as_u16() < 400 {
        return false;
    }
    if let HttpBody::Bytes(bytes) = &response.body {
        return overloaded_json(bytes);
    }
    let HttpBody::Stream(mut stream) =
        std::mem::replace(&mut response.body, HttpBody::Bytes(Bytes::new()))
    else {
        unreachable!()
    };
    let mut chunks = Vec::new();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        match &chunk {
            Ok(data) if bytes.len().saturating_add(data.len()) <= 64 * 1024 => {
                bytes.extend_from_slice(data)
            }
            _ => {
                chunks.push(chunk);
                response.body =
                    HttpBody::Stream(Box::pin(futures_util::stream::iter(chunks).chain(stream)));
                return false;
            }
        }
        chunks.push(chunk);
    }
    let result = overloaded_json(&bytes);
    response.body = HttpBody::Bytes(Bytes::from(bytes));
    result
}

async fn save(
    store: &dyn ChannelState,
    key: &str,
    state: &Session,
    version: &mut Option<Version>,
) -> Result<bool, ChannelError> {
    let until = [
        state
            .slow_reset_ms
            .map(|reset| reset.saturating_add(60_000)),
        state.budget_until_ms,
        state.cooloff_until_ms,
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(unix_now_ms().saturating_add(24 * 60 * 60 * 1000));
    let payload =
        serde_json::to_vec(state).map_err(|error| super::invalid_response(error.to_string()))?;
    if let Ok(CasResult::Applied(next)) = store
        .compare_exchange(
            key,
            version.clone(),
            Some(StateWrite {
                payload: Bytes::from(payload),
                expires_at: Some(
                    SystemTime::UNIX_EPOCH + Duration::from_millis(until.max(0) as u64),
                ),
            }),
        )
        .await
    {
        *version = next;
        return Ok(true);
    }
    Ok(false)
}

pub(super) async fn send(
    config: &ClaudecodeConfig,
    client: &dyn OutboundClient,
    store: &dyn ChannelState,
    session: &str,
    request: Request<HttpBody>,
    explicit: bool,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    if !config.low_priority && !config.extended_usage && !explicit {
        return Ok(client.send(request).await?);
    }
    let key = format!("session:{session}:usage_limit");
    let stored = store.get(&key).await.ok().flatten();
    let mut state: Session = stored
        .as_ref()
        .and_then(|entry| serde_json::from_slice(&entry.payload).ok())
        .unwrap_or_default();
    let mut saved = state.clone();
    let mut version = stored.map(|entry| entry.version);
    let started = Instant::now();
    let total = store.limits().operation_total;
    let (mut parts, body) = request.into_parts();
    let value = match &body {
        HttpBody::Bytes(bytes) => serde_json::from_slice::<Value>(bytes).ok(),
        HttpBody::Stream(_) => None,
    };
    // A fallback credit is an opaque redemption, not an ordinary generation
    // body. Preserve its existing single-use forwarding path.
    let credit = value
        .as_ref()
        .and_then(|value| value.get("fallback_credit_token"))
        .is_some_and(|value| !value.is_null());
    let replay = match &body {
        HttpBody::Bytes(bytes) if !credit => Some(bytes.clone()),
        _ => None,
    };
    let main = value
        .as_ref()
        .and_then(|body| body.get("system").and_then(Value::as_array))
        .is_none_or(|blocks| {
            !blocks.iter().any(|block| {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| {
                        text.starts_with("x-anthropic-billing-header:")
                            && text.contains(" cc_is_subagent=true;")
                    })
            })
        });
    let mut mode = text(&parts.headers, HEADER).and_then(Mode::parse);
    if explicit && mode == Some(Mode::Extended) {
        state.slow_reset_ms = None;
    }
    if !explicit {
        if config.low_priority && state.active(unix_now_ms()) {
            mode = Some(Mode::Slow);
        } else if mode == Some(Mode::Extended) && state.extra_usage_enabled == Some(true) {
            mode = None;
        }
    }
    set_mode(&mut parts.headers, mode);
    let mut request = Request::from_parts(parts.clone(), body);
    let mut waiting = Wait::default();
    if let Some(milliseconds) = state.retry_after_ms {
        waiting.retry = Duration::from_millis(milliseconds);
    }
    if let Some(milliseconds) = state.max_wait_ms {
        waiting.maximum = Duration::from_millis(milliseconds);
    }
    loop {
        let mut response = client.send(request).await?;
        let now = unix_now_ms();
        let stopped = state.observe(&response, mode, now, main);
        let mut retry = false;
        let mut delay = None;
        if replay.is_some() {
            if !explicit
                && mode != Some(Mode::Slow)
                && config.low_priority
                && started.elapsed() < total
                && state.accept_offer(&response, now)
            {
                mode = Some(Mode::Slow);
                waiting = Wait::default();
                waiting.observe(&response.headers);
                retry = true;
            } else if mode == Some(Mode::Slow) && !stopped {
                waiting.observe(&response.headers);
                let status = text(&response.headers, SLOW_STATUS);
                let busy =
                    response.status == StatusCode::TOO_MANY_REQUESTS && status == Some("slot_busy");
                let capacity =
                    matches!(status, None | Some("active")) && overloaded(&mut response).await;
                if busy || capacity {
                    delay = waiting.delay(total.saturating_sub(started.elapsed()));
                    retry = delay.is_some();
                    if !retry {
                        state.slow_reset_ms = None;
                        state.cooloff_until_ms = Some(now.saturating_add(10 * 60 * 1000));
                    }
                }
            }
        }
        if mode == Some(Mode::Slow) {
            state.retry_after_ms = Some(waiting.retry.as_millis() as u64);
            state.max_wait_ms = Some(waiting.maximum.as_millis() as u64);
        }
        if state != saved && save(store, &key, &state, &mut version).await? {
            saved = state.clone();
        }
        if !retry {
            return Ok(response);
        }
        if let Some(delay) = delay {
            sleep(delay).await;
            if started.elapsed() >= total
                || waiting
                    .since
                    .is_some_and(|since| since.elapsed() >= waiting.maximum)
            {
                state.slow_reset_ms = None;
                state.cooloff_until_ms = Some(unix_now_ms().saturating_add(10 * 60 * 1000));
                save(store, &key, &state, &mut version).await?;
                return Ok(response);
            }
            if state.slow_reset_ms.is_some() && !state.active(unix_now_ms()) {
                mode = None;
            }
        }
        set_mode(&mut parts.headers, mode);
        request = Request::from_parts(
            parts.clone(),
            HttpBody::Bytes(
                replay
                    .as_ref()
                    .expect("retry requires replay bytes")
                    .clone(),
            ),
        );
    }
}
