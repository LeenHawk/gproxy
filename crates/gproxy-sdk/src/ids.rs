//! Opaque identifiers for rows the handle creates itself, byte-for-byte the
//! same generator core uses for the ids it mints.

pub(crate) fn random_id() -> String {
    let mut bytes = [0u8; 16];
    // Entropy failure is not recoverable in a meaningful way here; a duplicate
    // id would surface as a Store constraint error rather than silent reuse.
    let _ = getrandom::fill(&mut bytes);
    let mut out = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
