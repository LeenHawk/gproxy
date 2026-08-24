use gproxy_channel_api::{
    Alpn, ClientProfile, Http2Profile, Http2Setting, PseudoHeader, TlsVersion,
};

pub(super) static CLIENT_PROFILE: ClientProfile = ClientProfile {
    alpn: &[Alpn::Http2, Alpn::Http1],
    min_tls_version: TlsVersion::Tls12,
    max_tls_version: TlsVersion::Tls13,
    cipher_list: concat!(
        "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:",
        "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305"
    ),
    curves_list: "X25519MLKEM768:X25519:P-256:P-384",
    sigalgs_list: Some(concat!(
        "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256:",
        "ecdsa_secp384r1_sha384:rsa_pss_rsae_sha384:rsa_pss_rsae_sha512"
    )),
    preserve_tls13_cipher_list: false,
    grease: true,
    http2: Some(Http2Profile {
        enable_push: false,
        initial_window_size: 6_291_456,
        initial_connection_window_size: 15_663_105,
        max_frame_size: 16_384,
        max_header_list_size: 262_144,
        pseudo_header_order: &[
            PseudoHeader::Method,
            PseudoHeader::Authority,
            PseudoHeader::Scheme,
            PseudoHeader::Path,
        ],
        settings_order: &[
            Http2Setting::EnablePush,
            Http2Setting::InitialWindowSize,
            Http2Setting::MaxFrameSize,
            Http2Setting::MaxHeaderListSize,
        ],
    }),
};
