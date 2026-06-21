//! Dual-target wall clock: `SystemTime` on native, `js_sys::Date` on wasm
//! (where `SystemTime::now` panics on wasm32-unknown-unknown).

/// Current unix time in seconds.
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Current unix time in seconds (JS host clock).
#[cfg(target_arch = "wasm32")]
pub fn unix_now() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

/// Current unix time in milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Current unix time in milliseconds (JS host clock).
#[cfg(target_arch = "wasm32")]
pub fn unix_now_ms() -> u64 {
    js_sys::Date::now() as u64
}

/// Async sleep abstracting the runtime timer. Native uses tokio; the wasm
/// gateway has no timer on the paths that call this (cookie/login retries are
/// native-only), so it is a no-op there.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

/// Async sleep — no-op on wasm (see the native variant).
#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(_ms: u64) {}
