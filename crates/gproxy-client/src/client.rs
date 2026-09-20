#[cfg(not(target_arch = "wasm32"))]
use crate::Backend;
use crate::{ConnectionConfig, Error};
#[cfg(all(
    any(feature = "reqwest", feature = "wreq", feature = "reqwest-native"),
    not(target_arch = "wasm32")
))]
use {
    crate::{ProxyConfig, RetryPolicy},
    std::time::Duration,
};

/// Shared clients retain the backends' native request, response and streaming APIs.
/// Cloning either backend handle shares its internal socket pool.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "reqwest")]
    Reqwest(reqwest::Client),
    #[cfg(feature = "wreq")]
    Wreq(wreq::Client),
    #[cfg(feature = "reqwest-native")]
    ReqwestNative(reqwest_native::Client),
    /// A transport the host built itself, handed to the pool through
    /// `ClientPool::with_client`. No profile setting applies to it: the host
    /// already decided what this client does.
    Host(std::sync::Arc<dyn crate::OutboundClient>),
}

/// wasm32 transports. `Backend` in the profile is a native choice; here every
/// profile resolves to the JS host's fetch (`Fetch` when the feature is on,
/// else the reqwest fallback) unless the host injected its own transport.
#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "fetch")]
    Fetch(crate::FetchClient),
    #[cfg(feature = "reqwest")]
    Reqwest(reqwest::Client),
    Host(std::sync::Arc<dyn crate::OutboundClient>),
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

#[cfg(target_arch = "wasm32")]
impl Client {
    #[cfg(feature = "fetch")]
    pub(crate) fn build(config: &ConnectionConfig, http1_only: bool) -> Result<Self, Error> {
        let _ = (config, http1_only);
        crate::FetchClient::new().map(Self::Fetch)
    }
    #[cfg(all(feature = "reqwest", not(feature = "fetch")))]
    pub(crate) fn build(config: &ConnectionConfig, http1_only: bool) -> Result<Self, Error> {
        build_reqwest(config, http1_only).map(Self::Reqwest)
    }
    #[cfg(not(any(feature = "fetch", feature = "reqwest")))]
    pub(crate) fn build(config: &ConnectionConfig, http1_only: bool) -> Result<Self, Error> {
        let _ = http1_only;
        Err(Error::BackendUnavailable(config.backend))
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Client {
    pub(crate) fn build(config: &ConnectionConfig, http1_only: bool) -> Result<Self, Error> {
        #[cfg(not(any(feature = "reqwest", feature = "wreq")))]
        let _ = http1_only;
        match config.backend {
            // The native-TLS backend has no WebSocket layer; its HTTP/1.1
            // (WebSocket) variant is the rustls reqwest client, like the
            // Codex CLI pairs native TLS for HTTP with rustls for WebSocket.
            Backend::ReqwestNative => {
                #[cfg(feature = "reqwest-native")]
                {
                    if http1_only {
                        build_reqwest(config, true).map(Self::Reqwest)
                    } else {
                        build_reqwest_native(config).map(Self::ReqwestNative)
                    }
                }
                #[cfg(not(feature = "reqwest-native"))]
                {
                    Err(Error::BackendUnavailable(Backend::ReqwestNative))
                }
            }
            Backend::Reqwest => {
                #[cfg(feature = "reqwest")]
                {
                    build_reqwest(config, http1_only).map(Self::Reqwest)
                }
                #[cfg(not(feature = "reqwest"))]
                {
                    Err(Error::BackendUnavailable(Backend::Reqwest))
                }
            }
            Backend::Wreq => {
                #[cfg(feature = "wreq")]
                {
                    build_wreq(config, http1_only).map(Self::Wreq)
                }
                #[cfg(not(feature = "wreq"))]
                {
                    Err(Error::BackendUnavailable(Backend::Wreq))
                }
            }
        }
    }
}

/// Fetch decides transport details on WASM: only the client handle is built.
#[cfg(all(feature = "reqwest", not(feature = "fetch"), target_arch = "wasm32"))]
fn build_reqwest(config: &ConnectionConfig, http1_only: bool) -> Result<reqwest::Client, Error> {
    let _ = (config, http1_only);
    reqwest::Client::builder().build().map_err(Error::Reqwest)
}

#[cfg(all(feature = "reqwest", not(target_arch = "wasm32")))]
fn build_reqwest(config: &ConnectionConfig, http1_only: bool) -> Result<reqwest::Client, Error> {
    // reqwest has no fingerprint control; silently sending a different identity
    // than the profile asked for would defeat the profile.
    if config.emulation.is_some() {
        return Err(Error::InvalidConfig("emulation requires the wreq backend"));
    }
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_millis(config.connect_timeout_ms.into()))
        .pool_idle_timeout(Duration::from_millis(config.pool_idle_timeout_ms.into()))
        .pool_max_idle_per_host(config.pool_max_idle_per_host as usize)
        .redirect(if config.redirect_max_hops == 0 {
            reqwest::redirect::Policy::none()
        } else {
            reqwest::redirect::Policy::limited(config.redirect_max_hops as usize)
        })
        .gzip(config.gzip)
        .brotli(config.brotli)
        .deflate(config.deflate)
        .zstd(config.zstd);
    // Leaving the builder untouched restores reqwest's native default classifier.
    if config.retry == RetryPolicy::Never {
        builder = builder.retry(reqwest::retry::never());
    }
    if http1_only {
        builder = builder.http1_only();
    }
    builder = match &config.proxy {
        ProxyConfig::Direct => builder.no_proxy(),
        ProxyConfig::System => builder,
        ProxyConfig::Explicit { url } => builder
            .no_proxy()
            .proxy(reqwest::Proxy::all(url).map_err(Error::Reqwest)?),
    };
    builder.build().map_err(Error::Reqwest)
}

/// reqwest 0.12 as the Codex CLI builds it: native TLS, no decompression
/// (the CLI enables none of reqwest's codec features) and no retry layer,
/// so a profile asking for either is refused rather than quietly ignored.
#[cfg(all(feature = "reqwest-native", not(target_arch = "wasm32")))]
fn build_reqwest_native(config: &ConnectionConfig) -> Result<reqwest_native::Client, Error> {
    if config.emulation.is_some() {
        return Err(Error::InvalidConfig("emulation requires the wreq backend"));
    }
    if config.gzip || config.brotli || config.deflate || config.zstd {
        return Err(Error::InvalidConfig(
            "the reqwest_native backend has no response decompression",
        ));
    }
    if config.retry != RetryPolicy::Never {
        return Err(Error::InvalidConfig(
            "the reqwest_native backend has no retry policy",
        ));
    }
    let mut builder = reqwest_native::Client::builder()
        .use_native_tls()
        .connect_timeout(Duration::from_millis(config.connect_timeout_ms.into()))
        .pool_idle_timeout(Duration::from_millis(config.pool_idle_timeout_ms.into()))
        .pool_max_idle_per_host(config.pool_max_idle_per_host as usize)
        .redirect(if config.redirect_max_hops == 0 {
            reqwest_native::redirect::Policy::none()
        } else {
            reqwest_native::redirect::Policy::limited(config.redirect_max_hops as usize)
        });
    builder = match &config.proxy {
        ProxyConfig::Direct => builder.no_proxy(),
        ProxyConfig::System => builder,
        ProxyConfig::Explicit { url } => builder
            .no_proxy()
            .proxy(reqwest_native::Proxy::all(url).map_err(Error::ReqwestNative)?),
    };
    builder.build().map_err(Error::ReqwestNative)
}

#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
fn build_wreq(config: &ConnectionConfig, http1_only: bool) -> Result<wreq::Client, Error> {
    let mut builder = wreq::Client::builder();
    if let Some(emulation) = &config.emulation {
        builder = emulation.apply(builder)?;
    }
    builder = builder
        .connect_timeout(Duration::from_millis(config.connect_timeout_ms.into()))
        .pool_idle_timeout(Duration::from_millis(config.pool_idle_timeout_ms.into()))
        .pool_max_idle_per_host(config.pool_max_idle_per_host as usize)
        .redirect(if config.redirect_max_hops == 0 {
            wreq::redirect::Policy::none()
        } else {
            wreq::redirect::Policy::limited(config.redirect_max_hops as usize)
        })
        .retry(match config.retry {
            RetryPolicy::Never => wreq::retry::Policy::never(),
            RetryPolicy::Default => wreq::retry::Policy::default(),
        })
        .gzip(config.gzip)
        .brotli(config.brotli)
        .deflate(config.deflate)
        .zstd(config.zstd);
    if http1_only {
        builder = builder.http1_only();
    }
    builder = match &config.proxy {
        ProxyConfig::Direct => builder.no_proxy(),
        ProxyConfig::System => builder,
        ProxyConfig::Explicit { url } => builder
            .no_proxy()
            .proxy(wreq::Proxy::all(url.as_str()).map_err(Error::Wreq)?),
    };
    builder.build().map_err(Error::Wreq)
}
