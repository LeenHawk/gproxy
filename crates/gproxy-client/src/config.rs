use std::fmt;

use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crate::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Reqwest,
    Wreq,
}

/// Native transport retries. Default restores each backend's safe protocol-NACK
/// retries (currently at most two); it does not add status-code/business retries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryPolicy {
    #[default]
    Never,
    Default,
}

/// Direct explicitly disables system proxy discovery. System uses the backend's
/// environment/OS discovery at construction; clear the pool after host proxy changes.
#[derive(Clone, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProxyConfig {
    #[default]
    Direct,
    System,
    Explicit {
        url: String,
    },
}

impl<'de> Deserialize<'de> for ProxyConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Struct variants make serde reject stray fields even for direct/system;
        // internally tagged unit variants otherwise silently discard them.
        #[derive(Deserialize)]
        #[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
        enum WireProxy {
            Direct {},
            System {},
            Explicit { url: String },
        }
        Ok(match WireProxy::deserialize(deserializer)? {
            WireProxy::Direct {} => Self::Direct,
            WireProxy::System {} => Self::System,
            WireProxy::Explicit { url } => Self::Explicit { url },
        })
    }
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct => f.write_str("Direct"),
            Self::System => f.write_str("System"),
            Self::Explicit { .. } => f.write_str("Explicit { url: [redacted] }"),
        }
    }
}

/// A named wreq-util TLS/HTTP profile, including platform and header behavior.
/// Strings use wreq-util's serde names, e.g. chrome_133 and linux. Unknown names
/// fail when constructing a wreq client; they never fall back to another fingerprint.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmulationConfig {
    pub profile: String,
    pub platform: String,
    pub http2: bool,
    pub headers: bool,
}

/// The profile portion of the cache key; WS also selects an HTTP/1.1 override.
/// IDs, names, API keys and destination URLs are deliberately
/// absent. Selecting a profile replaces the whole configuration, without merging.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConnectionConfig {
    pub backend: Backend,
    pub proxy: ProxyConfig,
    pub emulation: Option<EmulationConfig>,
    /// Enable automatic response decompression for each content encoding.
    /// Defaults preserve encoded response bytes and headers.
    pub gzip: bool,
    pub brotli: bool,
    pub deflate: bool,
    pub zstd: bool,
    /// Zero returns redirects as responses; positive values follow up to this many hops.
    pub redirect_max_hops: u32,
    pub retry: RetryPolicy,
    pub connect_timeout_ms: u32,
    /// Zero disables retention of idle connections.
    pub pool_idle_timeout_ms: u32,
    /// Idle connections per host, not a concurrency limit. Zero disables retention.
    pub pool_max_idle_per_host: u32,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            backend: Backend::Reqwest,
            proxy: ProxyConfig::Direct,
            emulation: None,
            gzip: false,
            brotli: false,
            deflate: false,
            zstd: false,
            redirect_max_hops: 0,
            retry: RetryPolicy::Never,
            connect_timeout_ms: 10_000,
            pool_idle_timeout_ms: 90_000,
            pool_max_idle_per_host: 32,
        }
    }
}

impl ConnectionConfig {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn normalized(mut self) -> Result<Self, Error> {
        if let ProxyConfig::Explicit { url } = &mut self.proxy {
            // Normalize proxy URLs to authority form without imposing additional
            // scheme, port or path restrictions on top of the URL parser.
            let parsed = url::Url::parse(url).map_err(Error::InvalidProxy)?;
            *url = parsed[..url::Position::BeforePath].to_owned();
        }
        if self.pool_idle_timeout_ms == 0 || self.pool_max_idle_per_host == 0 {
            self.pool_idle_timeout_ms = 0;
            self.pool_max_idle_per_host = 0;
        }
        Ok(self)
    }
}

#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
impl EmulationConfig {
    pub(crate) fn build(&self) -> Result<wreq_util::Emulation, Error> {
        use serde::de::value::{Error as ValueError, StrDeserializer};
        let profile =
            wreq_util::Profile::deserialize(StrDeserializer::<ValueError>::new(&self.profile))
                .map_err(|_| Error::InvalidConfig("unknown emulation profile"))?;
        let platform =
            wreq_util::Platform::deserialize(StrDeserializer::<ValueError>::new(&self.platform))
                .map_err(|_| Error::InvalidConfig("unknown emulation platform"))?;
        Ok(wreq_util::Emulation::builder()
            .profile(profile)
            .platform(platform)
            .http2(self.http2)
            .headers(self.headers)
            .build())
    }
}
