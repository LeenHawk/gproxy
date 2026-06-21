//! Built-in TLS impersonation profile for the `geminicli` channel — the real
//! Gemini CLI model-path fingerprint (system OpenSSL; JA4
//! `t13d521100_b262b3658495_8e6e362c5eac`, HTTP/1.1). See
//! `docs/agent-tls-fingerprints.md` §5. Applied when no DB `tls_fingerprint`
//! overrides it. The user-agent (model-templated) is injected per-request in
//! [`super::auth`]. Native + `upstream-wreq` only.
//!
//! Fidelity note: the real client is full system OpenSSL (52 ciphers); BoringSSL
//! cannot reproduce that set, so this is a best-effort AEAD subset (JA4 won't
//! match exactly — see the doc's BoringSSL fidelity note).

use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};

pub(super) fn default_emulation() -> wreq::Emulation {
    let tls = TlsOptions::builder()
        .alpn_protocols(Vec::<AlpnProtocol>::new())
        .grease_enabled(false)
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .cipher_list(
            "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:\
             ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:\
             ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:\
             ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305"
                .to_owned(),
        )
        .curves_list("X25519MLKEM768:X25519:P-256:P-384:P-521".to_owned())
        .build();
    wreq::Emulation::builder()
        .tls_options(tls)
        .build(wreq::Group::default())
}
