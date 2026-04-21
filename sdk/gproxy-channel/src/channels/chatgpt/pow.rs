//! Proof-of-work solver for `/backend-api/sentinel/chat-requirements/finalize`.
//!
//! Translated from `target/samples/output/pow.js` (itself reverse-engineered
//! from chatgpt.com's 2026-04 bundle). The hash is a 32-bit FNV-1a style
//! function with an avalanche mixer applied to `seed + base64(json(config))`.

use super::prepare_p::{ConfigOptions, build_base_config, encode_config};

pub const ERROR_PREFIX: &str = "wQ8Lk5FbGpA2NcR9dShT6gYjU7VxZ4D";
const MAX_ATTEMPTS: u64 = 500_000;

/// 32-bit FNV-1a + mix (xorshift-multiply). Matches `powHashHex` in pow.js.
pub fn pow_hash_hex(input: &str) -> String {
    let mut h: u32 = 2_166_136_261;
    for byte in input.bytes() {
        h ^= byte as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h ^= h >> 16;
    h = h.wrapping_mul(2_246_822_507);
    h ^= h >> 13;
    h = h.wrapping_mul(3_266_489_909);
    h ^= h >> 16;
    format!("{:08x}", h)
}

/// Solve PoW given `seed` and hex `difficulty` (e.g. `"061a80"`).
/// Returns `gAAAAAB<payload>~S`.
pub fn solve_pow(seed: &str, difficulty: &str, opts: &ConfigOptions) -> String {
    let base = build_base_config(opts);
    let dlen = difficulty.len();
    for attempt in 0..MAX_ATTEMPTS {
        let mut cfg = base.clone();
        cfg.attempt = attempt as f64;
        cfg.elapsed_ms = opts.elapsed_ms;
        let payload = encode_config(&cfg);
        let mut buf = String::with_capacity(seed.len() + payload.len());
        buf.push_str(seed);
        buf.push_str(&payload);
        let digest = pow_hash_hex(&buf);
        if digest[..dlen] <= *difficulty {
            return format!("gAAAAAB{}~S", payload);
        }
    }
    format!("{}{}", ERROR_PREFIX, base64_encode_bytes(b"e"))
}

fn base64_encode_bytes(b: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::chatgpt::prepare_p::ConfigOptions;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::Value;

    fn decode_payload(answer: &str) -> Vec<Value> {
        let p = answer
            .strip_prefix("gAAAAAB")
            .unwrap()
            .strip_suffix("~S")
            .unwrap();
        let bytes = STANDARD.decode(p).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn solves_sample_0() {
        let opts = ConfigOptions::fixed_for_tests();
        let answer = solve_pow("0.5099912974590367", "061a80", &opts);
        let cfg = decode_payload(&answer);
        assert_eq!(cfg.len(), 25);

        let payload = answer
            .strip_prefix("gAAAAAB")
            .unwrap()
            .strip_suffix("~S")
            .unwrap();
        let mut buf = String::new();
        buf.push_str("0.5099912974590367");
        buf.push_str(payload);
        let h = pow_hash_hex(&buf);
        assert!(
            &h[..6] <= "061a80",
            "hash {h} must satisfy difficulty 061a80"
        );
    }

    #[test]
    fn solves_sample_1() {
        let opts = ConfigOptions::fixed_for_tests();
        let answer = solve_pow("0.6287679384217534", "06c164", &opts);
        let payload = answer
            .strip_prefix("gAAAAAB")
            .unwrap()
            .strip_suffix("~S")
            .unwrap();
        let mut buf = String::new();
        buf.push_str("0.6287679384217534");
        buf.push_str(payload);
        let h = pow_hash_hex(&buf);
        assert!(
            &h[..6] <= "06c164",
            "hash {h} must satisfy difficulty 06c164"
        );
    }

    #[test]
    fn hash_matches_js_reference() {
        assert_eq!(pow_hash_hex("hello"), "888d766e");
        assert_eq!(pow_hash_hex(""), "ab3e7c0b");
    }
}
