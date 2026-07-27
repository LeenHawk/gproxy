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

/// Async sleep abstracting the runtime timer.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep_ms(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
export function gproxySleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_name = gproxySleep)]
    fn sleep(ms: u32) -> js_sys::Promise;
}

#[cfg(target_arch = "wasm32")]
pub async fn sleep_ms(ms: u64) {
    let _ = wasm_bindgen_futures::JsFuture::from(sleep(ms.min(u32::MAX as u64) as u32)).await;
}
