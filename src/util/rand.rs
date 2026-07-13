//! Cross-target randomness. One source for DEK/nonce/PKCE verifiers/ids so the
//! channel + crypto layers don't depend on a cipher crate re-exporting an RNG
//! (chacha20poly1305 0.11 dropped its `aead::OsRng` re-export). `getrandom`
//! uses the OS backend on native and the `js` backend on wasm.

/// Fill `buf` with cryptographically secure random bytes. Panics only if the
/// platform RNG is unavailable, which is unrecoverable for our use.
pub fn fill(buf: &mut [u8]) {
    getrandom::getrandom(buf).expect("platform RNG unavailable");
}

/// A fresh array of `N` random bytes.
pub fn bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    fill(&mut b);
    b
}

/// A URL-safe random password of at least 24 chars (CSPRNG). Used for the
/// first-boot admin (§14.2): 24 random bytes → 32 url-safe base64 chars.
pub fn password() -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes::<24>())
}

/// A fresh user API key: `sk-` + 32 CSPRNG bytes as url-safe base64 (43
/// chars). Keys are generated server-side ONLY — the admin create endpoint
/// never accepts caller key material (import is the sole external-key path).
pub fn api_key() -> String {
    use base64::Engine as _;
    format!(
        "sk-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes::<32>())
    )
}

/// A random RFC-4122 v4 UUID string (`8-4-4-4-12` hex). Cross-target and
/// cryptographically random — replaces the `uuid` crate (native-only) and its
/// weak `Date::now()` wasm fallback, so session/request ids are unpredictable
/// on edge too.
pub fn uuid_v4() -> String {
    uuid_v4_from(&bytes::<16>())
}

/// A fresh RFC-9562 UUIDv7 string: 48-bit unix-ms timestamp followed by
/// cryptographically random bits.
pub fn uuid_v7() -> String {
    let mut b = bytes::<16>();
    let ms = crate::util::time::unix_now_ms() & 0x0000_ffff_ffff_ffff;
    b[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
    b[6] = (b[6] & 0x0f) | 0x70;
    b[8] = (b[8] & 0x3f) | 0x80;
    format_uuid(&b)
}

/// Format 16 seed bytes as an RFC-4122 v4 UUID string, forcing the version (4)
/// and variant (1) bits. Use with the high 16 bytes of a hash to derive a
/// *deterministic* v4-shaped id (e.g. a session id from a conversation digest).
pub fn uuid_v4_from(seed: &[u8; 16]) -> String {
    let mut b = *seed;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // variant 1
    format_uuid(&b)
}

fn format_uuid(b: &[u8; 16]) -> String {
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn uuid_v7_has_timestamp_version_and_variant() {
        let id = super::uuid_v7();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "7");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }
}
