#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
use gproxy_client::EmulationConfig;
#[cfg(not(target_arch = "wasm32"))]
use gproxy_client::{Backend, ClientPool};
use gproxy_client::{ConnectionConfig, ProxyConfig};

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
        emulation: Some(EmulationConfig {
            profile: "chrome_133".into(),
            platform: "linux".into(),
            http2: true,
            headers: false,
        }),
        ..Default::default()
    };
    let pool = ClientPool::default();
    pool.get(&config).await.unwrap();
    config.emulation.as_mut().unwrap().profile = "unknown_browser".into();
    assert!(pool.get(&config).await.is_err());
    config.emulation.as_mut().unwrap().profile = "chrome_133".into();
    config.emulation.as_mut().unwrap().platform = "unknown_os".into();
    assert!(pool.get(&config).await.is_err());
}
