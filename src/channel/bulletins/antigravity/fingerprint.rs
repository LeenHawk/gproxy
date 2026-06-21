//! Built-in TLS impersonation profile for the `antigravity` channel — the real
//! Antigravity CLI model-path fingerprint (Go `crypto/tls`; JA4
//! `t13d131100_f57a46bbacb6_ab7e3b40a677`, HTTP/1.1, no ALPN). See
//! `docs/agent-tls-fingerprints.md` §5. Applied when no DB `tls_fingerprint`
//! overrides it. The user-agent is injected per-request in [`super::auth`].
//! Native + `upstream-wreq` only.

use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};

pub(super) fn default_emulation() -> wreq::Emulation {
    let tls = TlsOptions::builder()
        .alpn_protocols(Vec::<AlpnProtocol>::new())
        .grease_enabled(false)
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        // Go lists the TLS 1.3 suites last; preserve that ordering.
        .cipher_list(
            "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:\
             ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:\
             ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:\
             ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:\
             ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:\
             TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256"
                .to_owned(),
        )
        .curves_list("X25519MLKEM768:X25519:P-256:P-384:P-521".to_owned())
        .sigalgs_list(
            "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519:\
             rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:rsa_pkcs1_sha256:\
             rsa_pkcs1_sha384:rsa_pkcs1_sha512:ecdsa_secp384r1_sha384:\
             ecdsa_secp521r1_sha512"
                .to_owned(),
        )
        .preserve_tls13_cipher_list(true)
        .build();
    wreq::Emulation::builder()
        .tls_options(tls)
        .build(wreq::Group::default())
}
