#[cfg(not(target_arch = "wasm32"))]
use crate::Backend;
use crate::{ConnectionConfig, Error};
#[cfg(all(
    any(feature = "reqwest", feature = "wreq"),
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
