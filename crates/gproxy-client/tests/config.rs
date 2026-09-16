use gproxy_client::{Backend, ConnectionConfig, EmulationConfig, Error, ProxyConfig};

#[test]
fn rejects_invalid_or_incompatible_configuration() {
    let mut config = ConnectionConfig {
        emulation: Some(EmulationConfig {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        }),
        ..Default::default()
    };
    assert!(matches!(config.validate(), Err(Error::InvalidConfig(_))));
    config.emulation = None;
    for url in [
        "",
        "ftp://proxy.test",
        "http://proxy.test/path",
        "http://proxy.test?x=1",
        "socks5://proxy.test",
        "http://proxy.test:0",
    ] {
        config.proxy = ProxyConfig::Explicit { url: url.into() };
        assert!(
            matches!(config.validate(), Err(Error::InvalidProxy)),
            "{url}"
        );
    }
    assert!(
        serde_json::from_str::<ConnectionConfig>(
            r#"{"proxy":{"mode":"direct","url":"http://ignored.test"}}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<ConnectionConfig>(r#"{"connect_timout_ms":5}"#).is_err());
}

#[test]
fn unavailable_backend_never_falls_back() {
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
        assert_eq!(config.validate().is_ok(), available);
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
#[test]
fn unknown_fingerprints_fail_instead_of_changing_identity() {
    let mut config = ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        }),
        ..Default::default()
    };
    config.validate().unwrap();
    config.emulation.as_mut().unwrap().profile = "unknown_browser".into();
    assert!(config.validate().is_err());
    config.emulation.as_mut().unwrap().profile = "chrome_133".into();
    config.emulation.as_mut().unwrap().platform = "unknown_os".into();
    assert!(config.validate().is_err());
}
