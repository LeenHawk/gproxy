//! Opaque identifiers for rows and contexts core creates itself.
//!
//! Some of these ids are capabilities, not just row keys: a publication
//! binding id is the unguessable half of a resource URL, and the identity
//! namespaces seed ids clients echo back. An id minted from a buffer the
//! entropy source never filled is the all-zero string, the same for every
//! caller, so a failed `getrandom` must never yield an id.
//!
//! Failure panics rather than returning a `Result`. The OS random source
//! failing is not something a request can recover from or a caller can
//! retry around; the generators are called from ~20 infallible-shaped sites
//! (row constructors, id prefixes, multipart boundaries) where threading an
//! error would only end in the same abort with more plumbing. The panic
//! unwinds the one task that needed an id, and fails closed.

/// Fills `bytes` from the OS random source, panicking when it cannot.
/// See the module docs for why this does not return an error.
pub(crate) fn fill_random(bytes: &mut [u8]) {
    if let Err(error) = getrandom::fill(bytes) {
        panic!("system random source unavailable, refusing to mint an id: {error}");
    }
}

pub(crate) fn random_id() -> String {
    let mut bytes = [0u8; 16];
    fill_random(&mut bytes);
    let mut out = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
