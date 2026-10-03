//! Opaque identifiers for rows the handle creates itself, byte-for-byte the
//! same generator core uses for the ids it mints.

pub(crate) fn random_id() -> String {
    let mut bytes = [0u8; 16];
    // Fail closed: a buffer the entropy source never filled would hex to the
    // same all-zero id for every caller, and some of these ids (login state,
    // login session ids) are capabilities. An OS random source failure is
    // not recoverable per request, and the callers are infallible-shaped, so
    // this panics the same way core's generator does.
    if let Err(error) = getrandom::fill(&mut bytes) {
        panic!("system random source unavailable, refusing to mint an id: {error}");
    }
    let mut out = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
