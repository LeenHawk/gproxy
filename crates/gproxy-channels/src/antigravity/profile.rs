use gproxy_channel_api::{ClientProfile, TlsVersion};

pub(super) static PROFILE: ClientProfile = ClientProfile {
    alpn: &[],
    min_tls_version: TlsVersion::Tls12,
    max_tls_version: TlsVersion::Tls13,
    cipher_list: concat!(
        "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
        "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
        "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:",
        "ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:",
        "ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:",
        "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256"
    ),
    curves_list: "X25519MLKEM768:X25519:P-256:P-384:P-521",
    sigalgs_list: Some(concat!(
        "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519:",
        "rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:rsa_pkcs1_sha256:",
        "rsa_pkcs1_sha384:rsa_pkcs1_sha512:ecdsa_secp384r1_sha384:",
        "ecdsa_secp521r1_sha512"
    )),
    preserve_tls13_cipher_list: true,
    grease: false,
    http2: None,
};
