//! Map a `tls_fingerprint` JSON value to a wreq [`Emulation`] (§7.4).
//!
//! Fingerprint schema (constrained → wreq): `{"headers"?: {name: value}, "http2"?:
//! {...}|false, "tls"?: {...}}`. The `headers` layer becomes the emulation's default
//! headers; the `tls` object maps to a [`wreq::tls::TlsOptions`] (ALPN, GREASE, TLS
//! version range, cipher/curve/sigalg BoringSSL token lists, extension permutation);
//! the `http2` object maps to a [`wreq::http2::Http2Options`] (SETTINGS values +
//! order, connection window, pseudo-header order) — the Akamai HTTP/2 fingerprint.
//! An unparsable/empty fingerprint yields no emulation (`None`) — the pool treats
//! that as a config error for a present fingerprint (never a silent TLS-layer drop).
//!
//! Keys per `docs/agent-tls-fingerprints.md` §5/§6. Keys prefixed `_` (`_reference`,
//! `_fidelity`, `_unsupported`) are comments and ignored. `"http2": false` (an HTTP/1.1
//! -only channel) is a no-op here — the `tls.alpn_protocols` list (which omits `h2`)
//! is what keeps the connection on HTTP/1.1; only an `http2` OBJECT emits an h2
//! fingerprint.
//!
//! BoringSSL fidelity ceiling (per the doc): the achievable target is an exact UA
//! plus ALPN, GREASE-off, and cipher/curve/sigalg approximation — NOT byte-exact
//! JA3/JA4 of OpenSSL/Go/rustls stacks. The structural mapping below is what is
//! verifiable offline; byte-exact JA3/JA4 needs a live capture against the binary.

use std::borrow::Cow;
use std::collections::BTreeMap;

use http::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use wreq::http2::{Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder};
use wreq::tls::{AlpnProtocol, ExtensionType, TlsOptions, TlsVersion};

/// blake3 hex of the canonicalized fingerprint — the pool cache key. Object keys
/// are recursively sorted so two semantically-equal fingerprints that differ only
/// in key insertion order hash identically (and thus share one upstream client).
pub fn fingerprint_hash(fp: &Value) -> String {
    let canonical = canonicalize(fp);
    // Canonical form serializes deterministically (BTreeMap → sorted keys).
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// Recursively rebuild `v` with every object's keys sorted, so serialization is
/// order-independent. Arrays keep their order (order is semantic there).
fn canonicalize(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let sorted: BTreeMap<String, Value> = map
                .iter()
                .map(|(k, val)| (k.clone(), canonicalize(val)))
                .collect();
            // serde_json::Map preserves BTreeMap's sorted iteration order on collect.
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

/// Build a wreq [`Emulation`] from the fingerprint, or `None` if nothing applies.
///
/// Maps the `headers` object → [`HeaderMap`], the `tls` object → [`TlsOptions`],
/// and the `http2` object → [`Http2Options`], applying only fields that are
/// present. Returns `None` when no layer yields anything usable.
pub fn to_emulation(fp: &Value) -> Option<wreq::Emulation> {
    let obj = fp.as_object()?;

    let headers = obj
        .get("headers")
        .and_then(headers_from_json)
        .filter(|hm| !hm.is_empty());

    let tls = obj.get("tls").and_then(tls_from_json);
    // `http2: false` (an HTTP/1.1-only channel) → None here; ALPN governs.
    let http2 = obj.get("http2").and_then(http2_from_json);

    // Nothing to emulate (empty / `http2: false`-only) → no client preset.
    if headers.is_none() && tls.is_none() && http2.is_none() {
        return None;
    }

    let mut builder = wreq::Emulation::builder();
    if let Some(headers) = headers {
        builder = builder.headers(headers);
    }
    if let Some(tls) = tls {
        builder = builder.tls_options(tls);
    }
    if let Some(http2) = http2 {
        builder = builder.http2_options(http2);
    }
    // `Group::default()` = no request-partitioning grouping; we only attach the
    // header + TLS + HTTP/2 layers to the client.
    Some(builder.build(wreq::Group::default()))
}

/// Parse a `{name: value}` JSON object into a [`HeaderMap`]. Non-string values and
/// names/values that aren't valid header tokens are skipped (best-effort).
fn headers_from_json(v: &Value) -> Option<HeaderMap> {
    let obj = v.as_object()?;
    let mut hm = HeaderMap::with_capacity(obj.len());
    for (name, val) in obj {
        let Some(val) = val.as_str() else { continue };
        let (Ok(hn), Ok(hv)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(val),
        ) else {
            continue;
        };
        hm.append(hn, hv);
    }
    Some(hm)
}

/// Parse the `tls` JSON object into [`TlsOptions`], applying only present keys.
///
/// All keys are optional (best-effort): unknown or malformed values are skipped, a
/// partial object still produces options. Returns `None` only if `v` is not an object
/// or carries no recognized TLS field at all.
fn tls_from_json(v: &Value) -> Option<TlsOptions> {
    let obj = v.as_object()?;
    let mut builder = TlsOptions::builder();
    let mut applied = false;

    // ALPN: array of protocol strings ("h2", "http/1.1", "h3"). An explicitly empty
    // array means "send no ALPN" — distinct from the field being absent.
    if let Some(arr) = obj.get("alpn_protocols").and_then(Value::as_array) {
        let alpn: Vec<AlpnProtocol> = arr
            .iter()
            .filter_map(Value::as_str)
            .filter_map(parse_alpn)
            .collect();
        builder = builder.alpn_protocols(alpn);
        applied = true;
    }

    if let Some(b) = obj.get("grease_enabled").and_then(Value::as_bool) {
        builder = builder.grease_enabled(b);
        applied = true;
    }

    if let Some(ver) = obj
        .get("min_tls_version")
        .and_then(Value::as_str)
        .and_then(parse_tls_version)
    {
        builder = builder.min_tls_version(ver);
        applied = true;
    }
    if let Some(ver) = obj
        .get("max_tls_version")
        .and_then(Value::as_str)
        .and_then(parse_tls_version)
    {
        builder = builder.max_tls_version(ver);
        applied = true;
    }

    // BoringSSL token lists carried verbatim (`:`-separated). Owned → 'static Cow.
    if let Some(s) = obj.get("cipher_list").and_then(Value::as_str) {
        builder = builder.cipher_list(s.to_owned());
        applied = true;
    }
    if let Some(s) = obj.get("curves_list").and_then(Value::as_str) {
        builder = builder.curves_list(s.to_owned());
        applied = true;
    }
    if let Some(s) = obj.get("sigalgs_list").and_then(Value::as_str) {
        builder = builder.sigalgs_list(s.to_owned());
        applied = true;
    }

    if let Some(b) = obj
        .get("preserve_tls13_cipher_list")
        .and_then(Value::as_bool)
    {
        builder = builder.preserve_tls13_cipher_list(b);
        applied = true;
    }

    // Extension ordering: array of extension type numbers (u16). Only able to permute
    // extensions BoringSSL actually emits (per the doc's fidelity note).
    if let Some(arr) = obj.get("extension_permutation").and_then(Value::as_array) {
        let exts: Vec<ExtensionType> = arr
            .iter()
            .filter_map(Value::as_u64)
            .filter_map(|n| u16::try_from(n).ok())
            .map(ExtensionType::from)
            .collect();
        if !exts.is_empty() {
            builder = builder.extension_permutation(Cow::Owned(exts));
            applied = true;
        }
    }

    applied.then(|| builder.build())
}

/// Map an ALPN protocol id string to the wreq [`AlpnProtocol`] constant. Unknown
/// protocols are dropped (best-effort).
fn parse_alpn(s: &str) -> Option<AlpnProtocol> {
    match s {
        "h2" => Some(AlpnProtocol::HTTP2),
        "http/1.1" => Some(AlpnProtocol::HTTP1),
        "h3" => Some(AlpnProtocol::HTTP3),
        _ => None,
    }
}

/// Parse a `"tls1.x"` string into a wreq [`TlsVersion`]. Accepts the spec's
/// `tls1.0`..`tls1.3` spelling (case-insensitive).
fn parse_tls_version(s: &str) -> Option<TlsVersion> {
    match s.to_ascii_lowercase().as_str() {
        "tls1.0" | "tls1" => Some(TlsVersion::TLS_1_0),
        "tls1.1" => Some(TlsVersion::TLS_1_1),
        "tls1.2" => Some(TlsVersion::TLS_1_2),
        "tls1.3" => Some(TlsVersion::TLS_1_3),
        _ => None,
    }
}

/// Parse the `http2` object into [`Http2Options`] — the Akamai HTTP/2 fingerprint
/// (SETTINGS values + their order, the connection-level window, pseudo-header
/// order). `http2: false` (a bare bool) is `None` (HTTP/1.1-only; ALPN governs).
/// Only present keys are applied; an object with no recognized key → `None`.
fn http2_from_json(v: &Value) -> Option<Http2Options> {
    let obj = v.as_object()?; // `false`/scalar → not an object → None
    let mut b = Http2Options::builder();
    let mut applied = false;

    if let Some(x) = obj.get("enable_push").and_then(Value::as_bool) {
        b = b.enable_push(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "initial_window_size") {
        b = b.initial_window_size(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "initial_connection_window_size") {
        b = b.initial_connection_window_size(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "max_frame_size") {
        b = b.max_frame_size(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "max_header_list_size") {
        b = b.max_header_list_size(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "header_table_size") {
        b = b.header_table_size(x);
        applied = true;
    }
    if let Some(x) = u32_field(obj, "max_concurrent_streams") {
        b = b.max_concurrent_streams(x);
        applied = true;
    }
    if let Some(order) = obj.get("headers_pseudo_order").and_then(pseudo_order_from) {
        b = b.headers_pseudo_order(order);
        applied = true;
    }
    if let Some(order) = obj.get("settings_order").and_then(settings_order_from) {
        b = b.settings_order(order);
        applied = true;
    }

    applied.then(|| b.build())
}

/// Read an object key as a `u32` (JSON numbers exceeding `u32::MAX` are skipped).
fn u32_field(obj: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    obj.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

/// `[":method", ":scheme", ...]` → [`PseudoOrder`]. Unknown entries are skipped.
fn pseudo_order_from(v: &Value) -> Option<PseudoOrder> {
    let ids: Vec<PseudoId> = v
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|s| match s {
            ":method" => Some(PseudoId::Method),
            ":scheme" => Some(PseudoId::Scheme),
            ":authority" => Some(PseudoId::Authority),
            ":path" => Some(PseudoId::Path),
            _ => None,
        })
        .collect();
    (!ids.is_empty()).then(|| PseudoOrder::builder().extend(ids).build())
}

/// `[2, 4, 5, 6]` (HTTP/2 SETTINGS identifiers) → [`SettingsOrder`].
fn settings_order_from(v: &Value) -> Option<SettingsOrder> {
    let ids: Vec<SettingId> = v
        .as_array()?
        .iter()
        .filter_map(Value::as_u64)
        .filter_map(|n| match n {
            1 => Some(SettingId::HeaderTableSize),
            2 => Some(SettingId::EnablePush),
            3 => Some(SettingId::MaxConcurrentStreams),
            4 => Some(SettingId::InitialWindowSize),
            5 => Some(SettingId::MaxFrameSize),
            6 => Some(SettingId::MaxHeaderListSize),
            8 => Some(SettingId::EnableConnectProtocol),
            9 => Some(SettingId::NoRfc7540Priorities),
            _ => None,
        })
        .collect();
    (!ids.is_empty()).then(|| SettingsOrder::builder().extend(ids).build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fingerprint_hash_canonical() {
        // Same content, different key insertion order (top-level and nested) → equal hash.
        let a = json!({
            "headers": {"user-agent": "x", "accept": "y"},
            "tls": {"min": 1, "max": 2}
        });
        let b = json!({
            "tls": {"max": 2, "min": 1},
            "headers": {"accept": "y", "user-agent": "x"}
        });
        assert_eq!(fingerprint_hash(&a), fingerprint_hash(&b));

        // Different content → different hash.
        let c = json!({"headers": {"user-agent": "z"}});
        assert_ne!(fingerprint_hash(&a), fingerprint_hash(&c));
    }

    // Literal §5 drafts from docs/agent-tls-fingerprints.md, trimmed to the mapped
    // keys (the `_*` comment keys are kept on a couple to prove they are ignored).

    fn claude_fp() -> Value {
        json!({
            "headers": { "user-agent": "claude-code/2.1.162" },
            "tls": {
                "alpn_protocols": ["http/1.1"],
                "grease_enabled": false,
                "min_tls_version": "tls1.2", "max_tls_version": "tls1.3",
                "cipher_list": "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256",
                "curves_list": "X25519:P-256:P-384",
                "sigalgs_list": "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256"
            },
            "_reference": { "ja4": "t13d1714h1_5b57614c22b0_43ade6aba3df" },
            "_fidelity": "high",
            "_unsupported": "JA4_c 可能小差。"
        })
    }

    fn agy_fp() -> Value {
        json!({
            "headers": { "user-agent": "codeium-language-server" },
            "tls": {
                "alpn_protocols": ["h2", "http/1.1"],
                "grease_enabled": false,
                "min_tls_version": "tls1.2", "max_tls_version": "tls1.3",
                "cipher_list": "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:TLS_AES_128_GCM_SHA256",
                "curves_list": "X25519MLKEM768:X25519:P-256:P-384:P-521",
                "sigalgs_list": "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519",
                "preserve_tls13_cipher_list": true
            },
            "_fidelity": "medium"
        })
    }

    fn kiro_fp() -> Value {
        json!({
            "headers": { "user-agent": "aws-sdk-rust/1.3.10 os/linux lang/rust/1.92.0" },
            "tls": {
                "alpn_protocols": [],
                "grease_enabled": false,
                "min_tls_version": "tls1.2", "max_tls_version": "tls1.3",
                "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
                "curves_list": "X25519:P-256:P-384",
                "sigalgs_list": "ecdsa_secp384r1_sha384:ecdsa_secp256r1_sha256:ed25519"
            },
            "_fidelity": "medium"
        })
    }

    fn codex_fp() -> Value {
        json!({
            "headers": { "user-agent": "codex_exec/0.137.0 (Debian 13.0.0; x86_64) xterm-256color" },
            "tls": {
                "alpn_protocols": ["h2"],
                "grease_enabled": false,
                "min_tls_version": "tls1.2", "max_tls_version": "tls1.3",
                "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES256-GCM-SHA384",
                "curves_list": "X25519:P-256:P-384"
            },
            "http2": {
                "enable_push": false,
                "initial_window_size": 2097152,
                "initial_connection_window_size": 5242880,
                "max_frame_size": 16384,
                "max_header_list_size": 16384,
                "headers_pseudo_order": [":method", ":scheme", ":authority", ":path"],
                "settings_order": [2, 4, 5, 6]
            },
            "_fidelity": "medium"
        })
    }

    fn gemini_fp() -> Value {
        json!({
            "headers": { "user-agent": "google-api-nodejs-client/9.15.1" },
            "tls": {
                "alpn_protocols": [],
                "grease_enabled": false,
                "min_tls_version": "tls1.2", "max_tls_version": "tls1.3",
                "cipher_list": "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256",
                "curves_list": "X25519MLKEM768:X25519:P-256:P-384:P-521"
            },
            "_fidelity": "low"
        })
    }

    #[test]
    fn channel_drafts_map_to_emulation() {
        for fp in [claude_fp(), agy_fp(), kiro_fp(), codex_fp(), gemini_fp()] {
            assert!(
                to_emulation(&fp).is_some(),
                "channel draft should produce an emulation: {fp}"
            );
        }
    }

    #[test]
    fn tls_options_reflect_json_fields() {
        let opts = tls_from_json(claude_fp().get("tls").unwrap()).expect("tls maps");
        assert_eq!(opts.grease_enabled, Some(false));
        assert_eq!(opts.min_tls_version, Some(TlsVersion::TLS_1_2));
        assert_eq!(opts.max_tls_version, Some(TlsVersion::TLS_1_3));
        assert_eq!(opts.curves_list.as_deref(), Some("X25519:P-256:P-384"));
        // Single-protocol ALPN parsed to the HTTP1 constant.
        let alpn = opts.alpn_protocols.expect("alpn present");
        assert_eq!(alpn.as_ref(), &[AlpnProtocol::HTTP1]);

        // agy: preserve_tls13_cipher_list + multi-ALPN.
        let agy = tls_from_json(agy_fp().get("tls").unwrap()).expect("agy maps");
        assert_eq!(agy.preserve_tls13_cipher_list, Some(true));
        assert_eq!(
            agy.alpn_protocols.unwrap().as_ref(),
            &[AlpnProtocol::HTTP2, AlpnProtocol::HTTP1]
        );

        // Empty ALPN array is honored as "no protocols" (Some but empty), not skipped.
        let kiro = tls_from_json(kiro_fp().get("tls").unwrap()).expect("kiro maps");
        assert_eq!(kiro.alpn_protocols.unwrap().len(), 0);
    }

    #[test]
    fn headers_only_fingerprint_still_maps() {
        let fp = json!({ "headers": { "user-agent": "x" } });
        assert!(to_emulation(&fp).is_some());
    }

    #[test]
    fn empty_and_unmapped_yield_none() {
        assert!(to_emulation(&json!({})).is_none());
        // `http2: false` (HTTP/1.1-only) and an http2 object with no recognized
        // key both contribute nothing.
        assert!(to_emulation(&json!({ "http2": false })).is_none());
        assert!(to_emulation(&json!({ "http2": { "anything": 1 } })).is_none());
        // tls object with no recognized keys → no options → None.
        assert!(to_emulation(&json!({ "tls": { "_note": "x" } })).is_none());
    }

    #[test]
    fn http2_object_maps_to_emulation() {
        // An http2 object alone yields an emulation (the Akamai h2 fingerprint).
        assert!(
            to_emulation(
                codex_fp()
                    .get("http2")
                    .map(|h| json!({ "http2": h }))
                    .as_ref()
                    .unwrap()
            )
            .is_some()
        );
        // The pseudo-header + settings orders parse from the codex draft.
        let http2 = codex_fp();
        let h2 = http2.get("http2").unwrap();
        assert!(pseudo_order_from(h2.get("headers_pseudo_order").unwrap()).is_some());
        assert!(settings_order_from(h2.get("settings_order").unwrap()).is_some());
        assert!(http2_from_json(h2).is_some());
    }
}
