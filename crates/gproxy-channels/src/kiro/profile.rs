use gproxy_channel_api::{ClientProfile, TlsVersion};

pub(super) static CLIENT_PROFILE: ClientProfile = ClientProfile {
    alpn: &[],
    min_tls_version: TlsVersion::Tls12,
    max_tls_version: TlsVersion::Tls13,
    cipher_list: concat!(
        "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:",
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:",
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:",
        "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305"
    ),
    curves_list: "X25519:P-256:P-384",
    sigalgs_list: Some(concat!(
        "ecdsa_secp384r1_sha384:ecdsa_secp256r1_sha256:ed25519:",
        "rsa_pss_rsae_sha512:rsa_pss_rsae_sha384:rsa_pss_rsae_sha256:",
        "rsa_pkcs1_sha512:rsa_pkcs1_sha384:rsa_pkcs1_sha256"
    )),
    preserve_tls13_cipher_list: false,
    grease: false,
    http2: None,
};
