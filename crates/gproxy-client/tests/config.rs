#[cfg(not(target_arch = "wasm32"))]
use gproxy_client::{Backend, ClientPool};
use gproxy_client::{ConnectionConfig, EmulationConfig, ProxyConfig};

#[test]
fn rejects_unknown_fields() {
    assert!(
        serde_json::from_str::<ConnectionConfig>(
            r#"{"proxy":{"mode":"direct","url":"http://ignored.test"}}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<ConnectionConfig>(r#"{"connect_timout_ms":5}"#).is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn unavailable_backend_never_falls_back() {
    let pool = ClientPool::default();
    for (backend, available) in [
        (
            Backend::Reqwest,
            cfg!(all(feature = "reqwest", not(target_arch = "wasm32"))),
        ),
        (
            Backend::Wreq,
            cfg!(all(feature = "wreq", not(target_arch = "wasm32"))),
        ),
    ] {
        let config = ConnectionConfig {
            backend,
            ..Default::default()
        };
        assert_eq!(pool.get(&config).await.is_ok(), available);
    }
}

#[test]
fn proxy_debug_does_not_expose_credentials() {
    let config = ConnectionConfig {
        proxy: ProxyConfig::Explicit {
            url: "http://alice:secret@proxy.test:8080".into(),
        },
        ..Default::default()
    };
    let debug = format!("{config:?}");
    assert!(!debug.contains("alice"));
    assert!(!debug.contains("secret"));
    assert!(!debug.contains("proxy.test"));
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<ConnectionConfig>(&json).unwrap(),
        config
    );
}

#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
#[tokio::test]
async fn unknown_fingerprints_fail_instead_of_changing_identity() {
    let mut config = ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig::Preset {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        }),
        ..Default::default()
    };
    let pool = ClientPool::default();
    pool.get(&config).await.unwrap();
    config.emulation = Some(EmulationConfig::Preset {
        profile: "unknown_browser".into(),
        platform: "linux".into(),
        http2: true,
        headers: false,
    });
    assert!(pool.get(&config).await.is_err());
    config.emulation = Some(EmulationConfig::Preset {
        profile: "chrome_133".into(),
        platform: "unknown_os".into(),
        http2: true,
        headers: false,
    });
    assert!(pool.get(&config).await.is_err());
}

fn custom_fingerprint() -> EmulationConfig {
    use gproxy_client::{
        Alpn, Fingerprint, Http2Setting, Http2Settings, PseudoHeader, StreamPriority, TlsVersion,
    };
    EmulationConfig::Custom(Fingerprint {
        alpn: vec![Alpn::Http2, Alpn::Http1],
        min_tls: Some(TlsVersion::Tls12),
        max_tls: Some(TlsVersion::Tls13),
        cipher_list: Some("TLS_AES_128_GCM_SHA256:ECDHE-ECDSA-AES128-GCM-SHA256".into()),
        curves_list: Some("X25519:P-256".into()),
        sigalgs_list: Some("ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256".into()),
        preserve_tls13_cipher_list: Some(false),
        grease: Some(false),
        ocsp_stapling: Some(true),
        signed_cert_timestamps: Some(true),
        http2: Some(Http2Settings {
            enable_push: Some(false),
            initial_window_size: Some(2_097_152),
            initial_connection_window_size: Some(5_242_880),
            max_frame_size: Some(16_384),
            max_header_list_size: Some(16_384),
            header_table_size: None,
            max_concurrent_streams: None,
            pseudo_header_order: Some(vec![
                PseudoHeader::Method,
                PseudoHeader::Scheme,
                PseudoHeader::Authority,
                PseudoHeader::Path,
            ]),
            settings_order: Some(vec![
                Http2Setting::EnablePush,
                Http2Setting::InitialWindowSize,
            ]),
            headers_priority: Some(StreamPriority {
                dependency_id: 0,
                weight: 255,
                exclusive: true,
            }),
        }),
        headers: Some(vec![
            ("User-Agent".into(), "probe/1.0".into()),
            ("Accept".into(), "*/*".into()),
        ]),
    })
}

#[test]
fn emulation_round_trips_both_shapes_and_the_legacy_flat_preset() {
    let preset = EmulationConfig::Preset {
        profile: "chrome_133".into(),
        platform: "linux".into(),
        http2: true,
        headers: false,
    };
    let json = serde_json::to_value(&preset).unwrap();
    assert_eq!(json["kind"], "preset");
    assert_eq!(
        serde_json::from_value::<EmulationConfig>(json).unwrap(),
        preset
    );
    // Rows written before `kind` existed are the flat preset object.
    let legacy = serde_json::json!({
        "profile": "chrome_133", "platform": "linux", "http2": true, "headers": false
    });
    assert_eq!(
        serde_json::from_value::<EmulationConfig>(legacy).unwrap(),
        preset
    );

    let custom = custom_fingerprint();
    let json = serde_json::to_value(&custom).unwrap();
    assert_eq!(json["kind"], "custom");
    assert_eq!(json["alpn"], serde_json::json!(["http2", "http1"]));
    assert_eq!(json["min_tls"], "tls12");
    assert_eq!(json["http2"]["pseudo_header_order"][0], "method");
    assert_eq!(
        serde_json::from_value::<EmulationConfig>(json).unwrap(),
        custom
    );
    // A custom fingerprint with every knob unset is valid and stays custom.
    assert!(matches!(
        serde_json::from_str::<EmulationConfig>(r#"{"kind":"custom"}"#).unwrap(),
        EmulationConfig::Custom(fingerprint) if fingerprint == Default::default()
    ));

    // Unknown fields fail for every shape; a config embeds it unchanged.
    for bad in [
        r#"{"kind":"custom","alpn":["http1"],"ja3":"x"}"#,
        r#"{"kind":"preset","profile":"chrome_133","platform":"linux","http2":true,"headers":false,"extra":1}"#,
        r#"{"profile":"chrome_133","platform":"linux","http2":true,"headers":false,"extra":1}"#,
        r#"{"kind":"other"}"#,
    ] {
        assert!(
            serde_json::from_str::<EmulationConfig>(bad).is_err(),
            "{bad}"
        );
    }
    let config = ConnectionConfig {
        emulation: Some(custom),
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(
        serde_json::from_str::<ConnectionConfig>(&json).unwrap(),
        config
    );
}

#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
#[tokio::test]
async fn custom_fingerprints_build_on_wreq() {
    let pool = ClientPool::default();
    let config = ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(custom_fingerprint()),
        ..Default::default()
    };
    pool.get(&config).await.unwrap();
    pool.get_websocket(&config).await.unwrap();
    let bad_header = ConnectionConfig {
        emulation: Some(EmulationConfig::Custom(gproxy_client::Fingerprint {
            headers: Some(vec![("bad header".into(), "x".into())]),
            ..Default::default()
        })),
        ..config
    };
    assert!(matches!(
        pool.get(&bad_header).await.unwrap_err().as_ref(),
        gproxy_client::Error::InvalidConfig(_)
    ));
}

#[cfg(all(feature = "reqwest-native", not(target_arch = "wasm32")))]
#[tokio::test]
async fn reqwest_native_builds_with_the_supported_profile_options() {
    let pool = ClientPool::default();
    let config = ConnectionConfig {
        backend: Backend::ReqwestNative,
        ..Default::default()
    };
    assert!(matches!(
        pool.get(&config).await.unwrap().as_ref(),
        gproxy_client::Client::ReqwestNative(_)
    ));
    assert!(
        matches!(
            pool.get_websocket(&config).await.unwrap().as_ref(),
            gproxy_client::Client::Reqwest(_)
        ),
        "WebSocket profiles of this backend are the rustls client"
    );
    for bad in [
        ConnectionConfig {
            gzip: true,
            ..config.clone()
        },
        ConnectionConfig {
            retry: gproxy_client::RetryPolicy::Default,
            ..config.clone()
        },
        ConnectionConfig {
            emulation: Some(custom_fingerprint()),
            ..config.clone()
        },
    ] {
        assert!(pool.get(&bad).await.is_ok());
    }
    assert_eq!(
        serde_json::to_value(Backend::ReqwestNative).unwrap(),
        "reqwest_native"
    );
}

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
#[tokio::test]
async fn reqwest_accepts_the_supported_part_of_emulation() {
    let pool = ClientPool::default();
    for emulation in [
        custom_fingerprint(),
        EmulationConfig::Preset {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        },
    ] {
        let config = ConnectionConfig {
            backend: Backend::Reqwest,
            emulation: Some(emulation),
            ..Default::default()
        };
        assert!(pool.get(&config).await.is_ok());
    }
}
