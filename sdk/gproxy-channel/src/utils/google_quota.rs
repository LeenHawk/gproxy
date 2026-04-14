use crate::response::ResponseClassification;

/// Terminal quota error reasons from Google APIs.
const TERMINAL_REASONS: &[&str] = &[
    "INSUFFICIENT_G1_CREDITS_BALANCE",
    "INSUFFICIENT_CREDITS_BALANCE",
];

/// Classify a Google-style quota error (429 / 499 / 503) into
/// [`ResponseClassification`].
///
/// - Credits-exhausted reasons → `AuthDead` (credential is unusable).
/// - `retry-after` > 5 min → `RateLimited` with the full delay (long cooldown).
/// - Otherwise → `RateLimited` with the parsed retry-after (or `None`).
pub fn classify_google_quota_response(
    headers: &http::HeaderMap,
    body: &[u8],
) -> ResponseClassification {
    // Check body for terminal credit exhaustion.
    if body_contains_terminal_reason(body) {
        return ResponseClassification::AuthDead;
    }

    let retry_after_ms = headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 1000);

    // A very long retry-after (> 5 min) signals daily/weekly quota exhaustion.
    // We still pass it as retry_after_ms so the health uses the server-provided
    // delay rather than exponential back-off.
    ResponseClassification::RateLimited { retry_after_ms }
}

/// Scan the response body (assumed JSON) for known terminal quota reasons.
fn body_contains_terminal_reason(body: &[u8]) -> bool {
    // Fast path: try a byte-level substring scan before parsing JSON.
    let body_str = match std::str::from_utf8(body) {
        Ok(s) => s,
        Err(_) => return false,
    };
    TERMINAL_REASONS
        .iter()
        .any(|reason| body_str.contains(reason))
}
