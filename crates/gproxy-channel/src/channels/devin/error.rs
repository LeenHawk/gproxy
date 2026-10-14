//! The upstream error taxonomy.
//!
//! This upstream wraps **transient** faults inside a 401/403 auth shell, so
//! the status code alone is actively misleading: a capacity throttle, a
//! backend "an internal error occurred (trace ID: …)" and a content-policy
//! block all arrive as `permission_denied`. Reading that at face value retires
//! a live credential over a momentary blip, which is the failure the reference
//! project spent months fixing (`samples/windsurfapi/src/devin-connect.js`,
//! `classifyUpstreamError`, and the same reasoning repeated on the unary path
//! in `src/devin-connect-catalog.js`).
//!
//! The classifier below is ported from that function and **keeps its match
//! order**, which is load-bearing in two places the reference calls out:
//!
//! * the hard rate-limit pattern (`"… rate limit … Resets in: 3h0m0s"`) must
//!   run before the capacity pattern, because the capacity pattern's `try
//!   again later` sub-pattern also matches that text and would turn a three
//!   hour account throttle into a retryable sixty second blip;
//! * the content-policy pattern must run before the auth branch, because that
//!   branch's `permission_denied` pattern would otherwise read a blocked
//!   prompt as a dead token.
//!
//! The reference matches with case-insensitive regular expressions. This crate
//! carries no regular-expression dependency, so each pattern is spelled out as
//! substring tests over the lowercased text; every alternative of every
//! original pattern is reproduced, and the comment on each branch names it.
//!
//! # What each class means downstream
//!
//! A class becomes a [`ChannelError::UpstreamResponse`] whose status is the
//! one that actually describes the fault rather than the shell it arrived in.
//! `mod.rs` hands that back to the host as an *answer*, so
//! `gproxy-core::execute::attempt::classify` reads the status:
//!
//! | Class | Status | `classify` | Effect |
//! |---|---|---|---|
//! | `ClientRequest` | 400 | `Final` | returned to the caller, never retried |
//! | `ContentBlocked` | 400 | `Final` | per-request rejection, credential untouched |
//! | `Unauthorized` | 401 | `Exclude` | the credential is out for this request |
//! | `ModelBlocked` | 403 | `Exclude` | account cannot run this selector |
//! | `RateLimited` / `QuotaExhausted` | 429 | `Exclude` + a rate-limit block |
//! | `UpstreamInternal` | 502 | `Exclude` | transient backend fault, try elsewhere |
//! | `Capacity` | 503 | `Exclude` | transient throttle, try elsewhere |
//!
//! The two 400s are the whole point of the taxonomy on the inbound side: they
//! are `Final`, so nothing counts them against the credential and nothing
//! retries them. The 502/503 pair is the point on the outbound side: a
//! transient fault no longer presents as the 401 the upstream wrapped it in,
//! so it reads as "try another credential", not as "this token is dead".

use http::StatusCode;
use serde_json::{Value, json};

use crate::channel::ChannelError;

/// What the upstream actually said, behind whatever status it said it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// The gRPC `internal` code. The reference records this as a **permanent
    /// client mistake** — a short fingerprint, or a gzipped request frame —
    /// which fails identically on every retry. Distinct from the transient
    /// backend fault below, which is a *message* and not a code.
    ClientRequest,
    /// A hard account throttle carrying its own Go-duration window, or a bare
    /// 429 / `resource_exhausted`.
    RateLimited,
    /// Capacity or high demand: transient, retryable, usually delivered in a
    /// 401/403 shell.
    Capacity,
    /// `"an internal error occurred (trace ID: …)"`: a transient **backend**
    /// fault, observed 3/3 while `GetUserStatus` passed on the same token.
    UpstreamInternal,
    /// The request content tripped the upstream's content policy. Arrives as
    /// `permission_denied`; the token is alive.
    ContentBlocked,
    /// The account is out of credit or quota.
    QuotaExhausted,
    /// A tier wall: the plan does not include this model.
    ModelBlocked,
    /// A genuine authentication failure.
    Unauthorized,
    /// Nothing matched. Treated as a transient upstream fault, which is what
    /// the reference's `UPSTREAM_ERROR` fallback does.
    Unknown,
}

impl ErrorClass {
    /// The status that describes this fault. See the module table.
    pub fn status(self) -> StatusCode {
        match self {
            Self::ClientRequest | Self::ContentBlocked => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::ModelBlocked => StatusCode::FORBIDDEN,
            Self::RateLimited | Self::QuotaExhausted => StatusCode::TOO_MANY_REQUESTS,
            Self::UpstreamInternal | Self::Unknown => StatusCode::BAD_GATEWAY,
            Self::Capacity => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The `error.type` an OpenAI client sees. The names are the reference's
    /// own codes so an operator can grep one vocabulary across both.
    pub fn label(self) -> &'static str {
        match self {
            Self::ClientRequest => "upstream_error",
            Self::RateLimited => "rate_limited",
            Self::Capacity => "capacity",
            Self::UpstreamInternal => "upstream_internal",
            Self::ContentBlocked => "content_blocked",
            Self::QuotaExhausted => "quota_exhausted",
            Self::ModelBlocked => "model_blocked",
            Self::Unauthorized => "unauthorized",
            Self::Unknown => "upstream_error",
        }
    }
}

/// One classified refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    pub class: ErrorClass,
    /// The Connect code, when the payload carried one.
    pub code: Option<String>,
    /// The upstream's own message, kept verbatim for the operator.
    pub message: String,
    /// The window a hard rate limit named, in milliseconds.
    pub reset_ms: Option<u64>,
}

impl Upstream {
    /// The OpenAI-shaped error body a client understands, carrying the
    /// upstream's own message and code so nothing is lost in translation.
    pub fn body(&self) -> Value {
        let mut error = json!({
            "message": self.message,
            "type": self.class.label(),
        });
        if let Some(code) = &self.code {
            error["code"] = json!(code);
        }
        if let Some(reset) = self.reset_ms {
            // Seconds, as `retry-after` is spelled everywhere else. The value
            // cannot reach core (a channel answer carries no header the host
            // reads for this), but an operator reading the body needs the real
            // window: retrying into a three hour throttle amplifies load.
            error["retry_after"] = json!(reset / 1000);
        }
        json!({ "error": error })
    }

    /// The carrier this channel returns. `mod.rs` delivers it as an answer so
    /// core classifies it by status; `quota.rs` returns it as an error,
    /// because an account read has no answer to deliver.
    pub fn into_error(self) -> ChannelError {
        let status = self.class.status();
        let body = serde_json::to_vec(&self.body()).unwrap_or_else(|_| self.message.into_bytes());
        ChannelError::UpstreamResponse {
            status,
            body: gproxy_protocol::connection::Bytes::from(body),
        }
    }
}

/// Read a Connect error payload. Unary errors are top-level `{code, message}`
/// and the `GetChatMessage` trailer is nested `{"error":{code, message}}`;
/// both shapes are accepted here so the transient signal survives whichever
/// one the upstream sends (`devin-connect-catalog.js`, the comment above its
/// best-effort parse).
pub fn payload(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return (None, None);
    };
    let error = value.get("error").unwrap_or(&value);
    let text = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    (text("code"), text("message"))
}

/// `3h0m0s` and friends, capped at six hours. The observed window is three,
/// and the ceiling guards against a garbage value
/// (`devin-connect.js::parseResetDuration`).
fn reset_duration(lowercased: &str) -> Option<u64> {
    let mut total = 0_u64;
    let mut digits = 0_u64;
    let mut seen = false;
    for character in lowercased.chars() {
        if let Some(digit) = character.to_digit(10) {
            digits = digits.saturating_mul(10).saturating_add(u64::from(digit));
            seen = true;
            continue;
        }
        if seen {
            let scale = match character {
                'h' => 3_600_000,
                'm' => 60_000,
                's' => 1000,
                _ => 0,
            };
            total = total.saturating_add(digits.saturating_mul(scale));
        }
        digits = 0;
        seen = false;
    }
    (total > 0).then(|| total.min(6 * 3_600_000))
}

/// True when `first` occurs somewhere before `second`, which is how the
/// reference's `insufficient.*(credit|quota)` style patterns read.
fn ordered(haystack: &str, first: &str, second: &str) -> bool {
    haystack
        .find(first)
        .and_then(|at| haystack[at + first.len()..].find(second))
        .is_some()
}

fn any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// Classify one upstream refusal. `code` is the Connect code when the payload
/// carried one, `status` the HTTP status when a non-2xx was seen.
///
/// **The order of the branches is the taxonomy.** See the module
/// documentation before moving one.
pub fn classify(message: &str, code: Option<&str>, status: Option<StatusCode>) -> Upstream {
    let message = message.trim();
    let lowercased = message.to_ascii_lowercase();
    let status = status.map(|status| status.as_u16());
    let build = |class: ErrorClass, fallback: &str, reset_ms: Option<u64>| Upstream {
        class,
        code: code.map(str::to_owned),
        message: if message.is_empty() {
            fallback.to_owned()
        } else {
            message.to_owned()
        },
        reset_ms,
    };

    // ── Transient first ───────────────────────────────────────────────────
    // The gRPC `internal` code is a permanent client mistake, not the
    // transient backend fault further down; it fails the same way every time,
    // so it must not be retried.
    if code == Some("internal") {
        return build(
            ErrorClass::ClientRequest,
            "devin: the upstream rejected the request itself",
            None,
        );
    }
    // `/(message |request )?rate limit(ed)?/` together with `/resets? in[:\s]/`:
    // a hard account throttle carrying a Go-duration window. Before the
    // capacity branch, whose `try again later` also matches this text.
    if any(&lowercased, &["rate limit"]) && any(&lowercased, &["resets in", "reset in"]) {
        let reset = reset_duration(&lowercased);
        return build(
            ErrorClass::RateLimited,
            "devin: message rate limit reached",
            reset,
        );
    }
    // An explicit 429 or `resource_exhausted` is authoritative and outranks
    // the text-based capacity heuristic below: retrying into a real rate limit
    // amplifies load on an already throttled account.
    if status == Some(429) || code == Some("resource_exhausted") {
        return build(ErrorClass::RateLimited, "devin: rate limited", None);
    }
    // `/high demand|try again later|currently (busy|overloaded|at capacity)|
    //   model is (busy|overloaded)|temporarily (busy|overloaded|unavailable)|
    //   server is busy|overloaded|(service|backend|model|server) (is )?
    //   (temporarily )?unavailable|capacity/`, or the `unavailable` code.
    // Bare `overloaded` and bare `capacity` subsume several alternatives; the
    // `unavailable` spellings are listed in full because the original pattern
    // requires a noun or `temporarily` in front of that word.
    if code == Some("unavailable")
        || any(
            &lowercased,
            &[
                "high demand",
                "try again later",
                "overloaded",
                "capacity",
                "currently busy",
                "model is busy",
                "server is busy",
                "temporarily busy",
                "temporarily unavailable",
                "service unavailable",
                "service is unavailable",
                "backend unavailable",
                "backend is unavailable",
                "model unavailable",
                "model is unavailable",
                "server unavailable",
                "server is unavailable",
            ],
        )
    {
        return build(
            ErrorClass::Capacity,
            "devin: model temporarily at capacity",
            None,
        );
    }
    // A valid session can trigger this with a particular tool description.
    // Treat it as a request error, not an authentication failure that benches
    // the credential (verified by changing only that description).
    if lowercased.contains("unable to process request due to an mcp configuration issue") {
        return build(
            ErrorClass::ClientRequest,
            "devin: incompatible tool configuration",
            None,
        );
    }
    // "an internal error occurred (trace ID: …)": a transient *backend* fault
    // even inside a 401/403 shell. Observed 3/3 on a live free account whose
    // `GetUserStatus` kept answering, which is what proves the token was alive.
    if lowercased.contains("internal error occurred") {
        return build(
            ErrorClass::UpstreamInternal,
            "devin: upstream internal error",
            None,
        );
    }
    // `/blocked by (our |the )?content policy|remove (sensitive|unsafe)
    //   content|content[_ ]policy/`. Before the auth branch: this arrives as
    // `permission_denied` and would otherwise read as a dead token, producing
    // a re-login storm and benching a live account over one blocked prompt.
    if any(
        &lowercased,
        &[
            "content policy",
            "content_policy",
            "remove sensitive content",
            "remove unsafe content",
        ],
    ) {
        return build(
            ErrorClass::ContentBlocked,
            "devin: request blocked by the upstream content policy",
            None,
        );
    }

    // ── Account state, then permanent ─────────────────────────────────────
    // `/insufficient.*(credit|quota|balance|funds)|out of (credit|quota)|
    //   quota.*exceeded|credit.*exhausted/`. Before the tier wall so
    // "insufficient credit" never reads as a free-tier upgrade prompt.
    if ["credit", "quota", "balance", "funds"]
        .iter()
        .any(|word| ordered(&lowercased, "insufficient", word))
        || any(&lowercased, &["out of credit", "out of quota"])
        || ordered(&lowercased, "quota", "exceeded")
        || ordered(&lowercased, "credit", "exhausted")
    {
        return build(
            ErrorClass::QuotaExhausted,
            "devin: account out of credit or quota",
            None,
        );
    }
    // `/\/upgrade|upgrade to access|insufficient.*entitlement|
    //   requires? .*(paid|pro|team|enterprise)/`. An entitlement is a tier
    // wall — the plan is missing, not the balance.
    if any(&lowercased, &["/upgrade", "upgrade to access"])
        || ordered(&lowercased, "insufficient", "entitlement")
        || ["paid", "pro", "team", "enterprise"]
            .iter()
            .any(|tier| ordered(&lowercased, "require", tier))
    {
        return build(
            ErrorClass::ModelBlocked,
            "devin: the model requires a paid entitlement",
            None,
        );
    }
    // Only here does a 401/403 mean what it says. Everything transient the
    // upstream dresses up in this shell has already been taken above.
    if status == Some(401)
        || status == Some(403)
        || code == Some("permission_denied")
        || code == Some("unauthenticated")
        || any(&lowercased, &["permission_denied", "unauthenticated"])
        || ordered(&lowercased, "invalid", "token")
    {
        return build(
            ErrorClass::Unauthorized,
            "devin: authentication failed",
            None,
        );
    }
    // The reference's trailing rate-limit arm, for a throttle with neither a
    // reset window nor a 429.
    if any(
        &lowercased,
        &[
            "rate limit",
            "rate_limit",
            "ratelimit",
            "too many requests",
            "resource_exhausted",
        ],
    ) {
        return build(ErrorClass::RateLimited, "devin: rate limited", None);
    }
    build(ErrorClass::Unknown, "devin: upstream error", None)
}

/// Classify a non-2xx reply whose body is a Connect error payload.
pub fn from_response(status: StatusCode, body: &[u8]) -> ChannelError {
    let (code, message) = payload(body);
    let message = message.unwrap_or_else(|| String::from_utf8_lossy(body).into_owned());
    classify(&message, code.as_deref(), Some(status)).into_error()
}

/// Classify a `GetChatMessage` end-of-stream trailer. The trailer arrives on a
/// 200, so there is no status to help and the text is all there is.
pub fn from_trailer(code: Option<&str>, message: &str) -> ChannelError {
    classify(message, code, None).into_error()
}

/// Flatten a classified refusal that arrived too late to be an answer. Once
/// content has been streamed the status is spent, and the only thing left to
/// carry is the text — [`ChannelError::UpstreamResponse`] displays its status
/// and not its body, so the reason would otherwise be lost to whoever reads
/// the failed stream.
pub fn as_stream_failure(error: ChannelError) -> ChannelError {
    match error {
        ChannelError::UpstreamResponse { status, body } => {
            let detail = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| {
                    let error = value.get("error")?;
                    let message = error.get("message").and_then(Value::as_str)?;
                    Some(match error.get("code").and_then(Value::as_str) {
                        Some(code) => format!("{code}: {message}"),
                        None => message.to_owned(),
                    })
                })
                .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
            ChannelError::InvalidResponse(format!("devin upstream error ({status}): {detail}"))
        }
        other => other,
    }
}
