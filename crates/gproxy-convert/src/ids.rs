//! Opaque identifiers and the clock.
//!
//! Some of these ids are capabilities, not just keys: the identity namespaces
//! seed ids clients echo back. An id minted from a buffer the entropy source
//! never filled is the all-zero string, the same for every caller, so a failed
//! `getrandom` panics rather than yield one.

/// Fills `bytes` from the system random source, panicking when it cannot.
pub fn fill_random(bytes: &mut [u8]) {
    if let Err(error) = getrandom::fill(bytes) {
        panic!("system random source unavailable, refusing to mint an id: {error}");
    }
}

pub fn random_id() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    let mut out = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
