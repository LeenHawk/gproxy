//! Built-in TLS + HTTP/2 impersonation profile for the `codex` channel — the
//! real Codex CLI model-path fingerprint (rustls/hyper; JA4
//! `t13d1011h2_61a7ad8aa9b6_3fcd1a44f3e3`, HTTP/2). See
//! `docs/agent-tls-fingerprints.md` §5. Applied when no DB `tls_fingerprint`
//! overrides it. The user-agent is injected per-request in [`super::auth`].
//! Native + `upstream-wreq` only.

use wreq::http2::{Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder};
use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};

pub(super) fn default_emulation() -> wreq::Emulation {
    let tls = TlsOptions::builder()
        .alpn_protocols(vec![AlpnProtocol::HTTP2])
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

    // Akamai `2:0;4:2097152;5:16384;6:16384|5177345|0|m,s,a,p`. The
    // connection window = 65535 + 5177345 increment.
    let http2 = Http2Options::builder()
        .enable_push(false)
        .initial_window_size(2_097_152)
        .initial_connection_window_size(5_242_880)
        .max_frame_size(16_384)
        .max_header_list_size(16_384)
        .headers_pseudo_order(
            PseudoOrder::builder()
                .extend([
                    PseudoId::Method,
                    PseudoId::Scheme,
                    PseudoId::Authority,
                    PseudoId::Path,
                ])
                .build(),
        )
        .settings_order(
            SettingsOrder::builder()
                .extend([
                    SettingId::EnablePush,
                    SettingId::InitialWindowSize,
                    SettingId::MaxFrameSize,
                    SettingId::MaxHeaderListSize,
                ])
                .build(),
        )
        .build();

    wreq::Emulation::builder()
        .tls_options(tls)
        .http2_options(http2)
        .build(wreq::Group::default())
}
