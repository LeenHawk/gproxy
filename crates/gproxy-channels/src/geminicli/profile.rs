use std::borrow::Cow;

use gproxy_channel_api::{ClientProfile, TlsVersion};

pub(super) static PROFILE: ClientProfile = ClientProfile {
    alpn: Some(Cow::Borrowed(&[])),
    min_tls_version: Some(TlsVersion::Tls12),
    max_tls_version: Some(TlsVersion::Tls13),
    cipher_list: Some(Cow::Borrowed(concat!(
        "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:",
        "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305"
    ))),
    curves_list: Some(Cow::Borrowed("X25519MLKEM768:X25519:P-256:P-384:P-521")),
    sigalgs_list: None,
    preserve_tls13_cipher_list: Some(false),
    grease: Some(false),
    extension_permutation: None,
    http2: None,
};
