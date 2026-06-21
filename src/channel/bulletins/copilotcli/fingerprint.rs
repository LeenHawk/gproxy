//! Built-in TLS impersonation profile for the `copilotcli` channel — the real
//! GitHub Copilot CLI model-path fingerprint (rustls; JA4
//! `t13d1011h1_61a7ad8aa9b6_*`, HTTP/1.1). See `docs/agent-tls-fingerprints.md`
//! §5. Applied when no DB `tls_fingerprint` overrides it. The user-agent is
//! injected per-request in [`super::auth`]. Native + `upstream-wreq` only.

use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};

pub(super) fn default_emulation() -> wreq::Emulation {
    let tls = TlsOptions::builder()
        .alpn_protocols(vec![AlpnProtocol::HTTP1])
        .grease_enabled(false)
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .cipher_list(
            "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:\
             ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:\
             ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:\
             ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305"
                .to_owned(),
        )
        .curves_list("X25519:P-256:P-384".to_owned())
        .build();
    wreq::Emulation::builder()
        .tls_options(tls)
        .build(wreq::Group::default())
}
