use std::borrow::Cow;

use gproxy_channel_api::{
    Alpn, ClientProfile, Http2Profile, Http2Setting, PseudoHeader, TlsVersion,
};

pub(super) static CLIENT_PROFILE: ClientProfile = ClientProfile {
    alpn: Some(Cow::Borrowed(&[Alpn::Http2])),
    min_tls_version: Some(TlsVersion::Tls12),
    max_tls_version: Some(TlsVersion::Tls13),
    cipher_list: Some(Cow::Borrowed(concat!(
        "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:",
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:",
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:",
        "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305"
    ))),
    curves_list: Some(Cow::Borrowed("X25519:P-256:P-384")),
    sigalgs_list: None,
    preserve_tls13_cipher_list: Some(false),
    grease: Some(false),
    extension_permutation: None,
    http2: Some(Http2Profile {
        enable_push: Some(false),
        initial_window_size: Some(2_097_152),
        initial_connection_window_size: Some(5_242_880),
        max_frame_size: Some(16_384),
        max_header_list_size: Some(16_384),
        header_table_size: None,
        max_concurrent_streams: None,
        pseudo_header_order: Some(Cow::Borrowed(&[
            PseudoHeader::Method,
            PseudoHeader::Scheme,
            PseudoHeader::Authority,
            PseudoHeader::Path,
        ])),
        settings_order: Some(Cow::Borrowed(&[
            Http2Setting::EnablePush,
            Http2Setting::InitialWindowSize,
            Http2Setting::MaxFrameSize,
            Http2Setting::MaxHeaderListSize,
        ])),
    }),
};
