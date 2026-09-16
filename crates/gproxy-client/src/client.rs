use crate::{Backend, ConnectionConfig, Error};
#[cfg(any(feature = "reqwest", feature = "wreq"))]
use {
    crate::{ProxyConfig, RetryPolicy},
    std::time::Duration,
};

/// Shared clients retain the backends' native request, response and streaming APIs.
/// Cloning either backend handle shares its internal socket pool.
#[derive(Clone)]
pub enum Client {
    #[cfg(feature = "reqwest")]
    Reqwest(reqwest::Client),
    #[cfg(feature = "wreq")]
    Wreq(wreq::Client),
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

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

#[cfg(feature = "reqwest")]
fn build_reqwest(config: &ConnectionConfig, http1_only: bool) -> Result<reqwest::Client, Error> {
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
            .proxy(reqwest::Proxy::all(url).map_err(|_| Error::InvalidProxy)?),
    };
    builder.build().map_err(Error::Reqwest)
}

#[cfg(feature = "wreq")]
fn build_wreq(config: &ConnectionConfig, http1_only: bool) -> Result<wreq::Client, Error> {
    let mut builder = wreq::Client::builder();
    if let Some(emulation) = &config.emulation {
        builder = builder.emulation(emulation.build()?);
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
            .proxy(wreq::Proxy::all(url.as_str()).map_err(|_| Error::InvalidProxy)?),
    };
    builder.build().map_err(Error::Wreq)
}
