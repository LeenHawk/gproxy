//! ULID generation (§15.1): a 128-bit, lexicographically-sortable id rendered as
//! 26 Crockford base32 chars — 48-bit unix-ms timestamp prefix + 80 random bits.
//!
//! Hand-rolled rather than the `ulid` crate so it reuses the single RNG source
//! ([`crate::util::rand`]) and the dual-target clock ([`crate::util::time`]),
//! sidestepping the crate's std-time / `rand` assumptions on wasm32. Sortable by
//! creation time: the 10-char time prefix is big-endian, so byte/string order
//! matches chronological order within the same millisecond resolution.

use crate::util::{rand, time};

/// Crockford base32 alphabet (no I, L, O, U — the canonical ULID alphabet).
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generate a fresh ULID string (26 chars). The first 10 chars encode the
/// 48-bit millisecond timestamp; the last 16 encode 80 random bits.
pub fn ulid() -> String {
    let ms = time::unix_now_ms() & 0x0000_FFFF_FFFF_FFFF; // low 48 bits
    let rand = rand::bytes::<10>();

    // Assemble the full 128-bit value as 16 bytes: 6 time bytes + 10 random.
    let mut bytes = [0u8; 16];
    bytes[0..6].copy_from_slice(&ms.to_be_bytes()[2..8]);
    bytes[6..16].copy_from_slice(&rand);

    encode_base32(&bytes)
}

/// Encode 16 bytes (128 bits) as 26 Crockford base32 chars. 26×5 = 130 bits, so
/// the top 2 bits of the first char are always zero (matches the ULID spec).
fn encode_base32(bytes: &[u8; 16]) -> String {
    // Pack the 128 bits into a u128, then peel 5 bits at a time from the top.
    let n = u128::from_be_bytes(*bytes);
    let mut out = [0u8; 26];
    for (i, slot) in out.iter_mut().enumerate() {
        // Char i covers bits [125 - 5i, 121 - 5i]; the first char only has 3
        // significant bits (130 - 128), so its high 2 bits are zero.
        let shift = 125_i32 - 5 * i as i32;
        let idx = if shift >= 0 {
            (n >> shift) & 0x1f
        } else {
            (n << (-shift)) & 0x1f
        };
        *slot = ALPHABET[idx as usize];
    }
    // SAFETY-free: ALPHABET is ASCII, so the buffer is valid UTF-8.
    String::from_utf8(out.to_vec()).expect("base32 alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_shape() {
        let id = ulid();
        assert_eq!(id.len(), 26, "ULID is 26 chars");
        assert!(
            id.bytes().all(|b| ALPHABET.contains(&b)),
            "all chars in Crockford alphabet: {id}"
        );
    }

    #[test]
    fn ulid_time_prefix_is_monotonic() {
        // Two ULIDs minted in order share a non-decreasing 10-char time prefix
        // (same-ms ties resolve by the random tail, which we don't constrain).
        let a = ulid();
        let b = ulid();
        assert!(
            a[..10] <= b[..10],
            "time prefix sorts chronologically: {a} {b}"
        );
    }

    #[test]
    fn ulid_is_unique() {
        let a = ulid();
        let b = ulid();
        assert_ne!(a, b, "80 random bits make collisions vanishingly unlikely");
    }
}
