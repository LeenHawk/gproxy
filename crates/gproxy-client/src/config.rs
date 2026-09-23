use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    #[default]
    Reqwest,
    Wreq,
    /// reqwest 0.12 over the platform's native TLS (feature `reqwest-native`,
    /// native targets only): the Codex CLI's HTTP stack. WebSocket upgrades
    /// for this backend use the rustls `Reqwest` client, as the CLI does.
    ReqwestNative,
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

/// The TLS/HTTP identity a wreq client presents: either a wreq-util preset or a
/// custom [`Fingerprint`]. Serialized internally tagged by `kind` (`preset` /
/// `custom`); the untagged preset object stored by earlier releases
/// (`profile`, `platform`, `http2`, `headers`) still deserializes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmulationConfig {
    /// A named wreq-util TLS/HTTP profile, including platform and header
    /// behavior. Strings use wreq-util's serde names, e.g. chrome_133 and
    /// linux. Unknown names fail when constructing a wreq client; they never
    /// fall back to another fingerprint.
    Preset {
        profile: String,
        platform: String,
        http2: bool,
        headers: bool,
    },
    /// Explicit TLS, HTTP/2 and default-header settings (a captured client).
    Custom(Fingerprint),
}

impl<'de> Deserialize<'de> for EmulationConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Tagged {
            Preset {
                profile: String,
                platform: String,
                http2: bool,
                headers: bool,
            },
            Custom(Fingerprint),
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Flat {
            profile: String,
            platform: String,
            http2: bool,
            headers: bool,
        }
        // The tagged form is tried first; a missing `kind` is the legacy flat
        // preset object.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Tagged(Tagged),
            Flat(Flat),
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Tagged(Tagged::Preset {
                profile,
                platform,
                http2,
                headers,
            })
            | Wire::Flat(Flat {
                profile,
                platform,
                http2,
                headers,
            }) => Self::Preset {
                profile,
                platform,
                http2,
                headers,
            },
            Wire::Tagged(Tagged::Custom(fingerprint)) => Self::Custom(fingerprint),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alpn {
    Http1,
    Http2,
    Http3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsVersion {
    Tls10,
    Tls11,
    Tls12,
    Tls13,
}

/// HTTP/2 pseudo-header fields, for [`Http2Settings::pseudo_header_order`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PseudoHeader {
    Method,
    Scheme,
    Authority,
    Path,
}

/// SETTINGS frame parameters, for [`Http2Settings::settings_order`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Http2Setting {
    HeaderTableSize,
    EnablePush,
    MaxConcurrentStreams,
    InitialWindowSize,
    MaxFrameSize,
    MaxHeaderListSize,
}

/// The stream dependency carried on outgoing HEADERS frames (RFC 9113 5.3).
/// `weight` is the wire byte, i.e. the protocol weight minus one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamPriority {
    pub dependency_id: u32,
    pub weight: u8,
    pub exclusive: bool,
}

/// Explicit HTTP/2 connection settings. Unset fields keep wreq's defaults;
/// the two orders are completed with the remaining ids in their default order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Http2Settings {
    pub enable_push: Option<bool>,
    pub initial_window_size: Option<u32>,
    pub initial_connection_window_size: Option<u32>,
    pub max_frame_size: Option<u32>,
    pub max_header_list_size: Option<u32>,
    pub header_table_size: Option<u32>,
    pub max_concurrent_streams: Option<u32>,
    pub pseudo_header_order: Option<Vec<PseudoHeader>>,
    pub settings_order: Option<Vec<Http2Setting>>,
    pub headers_priority: Option<StreamPriority>,
}

/// A custom wreq client identity. Every field is optional; unset fields keep
/// wreq's defaults (which are not a browser's). The cipher, curve and
/// signature-algorithm strings use BoringSSL's colon-separated syntax.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fingerprint {
    /// ALPN protocols offered, in order. Empty keeps wreq's default offer.
    pub alpn: Vec<Alpn>,
    pub min_tls: Option<TlsVersion>,
    pub max_tls: Option<TlsVersion>,
    pub cipher_list: Option<String>,
    pub curves_list: Option<String>,
    pub sigalgs_list: Option<String>,
    /// Keep the TLS 1.3 suites of `cipher_list` in the given order instead of
    /// BoringSSL's fixed TLS 1.3 preference.
    pub preserve_tls13_cipher_list: Option<bool>,
    pub grease: Option<bool>,
    /// Offer the `status_request` (OCSP stapling) extension.
    pub ocsp_stapling: Option<bool>,
    /// Offer the `signed_certificate_timestamp` extension.
    pub signed_cert_timestamps: Option<bool>,
    pub http2: Option<Http2Settings>,
    /// Default request headers in send order, with their original casing.
    /// Request headers with the same name replace them.
    pub headers: Option<Vec<(String, String)>>,
}

/// The profile portion of the cache key; WS also selects an HTTP/1.1 override.
/// IDs, names, API keys and destination URLs are deliberately
/// absent. Selecting a profile replaces the whole configuration, without merging.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConnectionConfig {
    pub backend: Backend,
    pub proxy: ProxyConfig,
    /// wreq applies the full identity. Other backends apply the supported
    /// subset of custom fingerprints; wreq-only preset identities are skipped.
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
    pub(crate) fn apply(&self, builder: wreq::ClientBuilder) -> Result<wreq::ClientBuilder, Error> {
        match self {
            Self::Preset {
                profile,
                platform,
                http2,
                headers,
            } => {
                use serde::de::value::{Error as ValueError, StrDeserializer};
                let profile =
                    wreq_util::Profile::deserialize(StrDeserializer::<ValueError>::new(profile))
                        .map_err(|_| Error::InvalidConfig("unknown emulation profile"))?;
                let platform =
                    wreq_util::Platform::deserialize(StrDeserializer::<ValueError>::new(platform))
                        .map_err(|_| Error::InvalidConfig("unknown emulation platform"))?;
                Ok(builder.emulation(
                    wreq_util::Emulation::builder()
                        .profile(profile)
                        .platform(platform)
                        .http2(*http2)
                        .headers(*headers)
                        .build(),
                ))
            }
            Self::Custom(fingerprint) => fingerprint.apply(builder),
        }
    }
}

#[cfg(all(feature = "wreq", not(target_arch = "wasm32")))]
impl Fingerprint {
    fn apply(&self, mut builder: wreq::ClientBuilder) -> Result<wreq::ClientBuilder, Error> {
        use wreq::http2::{
            Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder, StreamDependency,
            StreamId,
        };
        use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion as WreqTlsVersion};

        let mut tls = TlsOptions::builder();
        if !self.alpn.is_empty() {
            tls = tls.alpn_protocols(self.alpn.iter().map(|alpn| match alpn {
                Alpn::Http1 => AlpnProtocol::HTTP1,
                Alpn::Http2 => AlpnProtocol::HTTP2,
                Alpn::Http3 => AlpnProtocol::HTTP3,
            }));
        }
        let version = |version: TlsVersion| match version {
            TlsVersion::Tls10 => WreqTlsVersion::TLS_1_0,
            TlsVersion::Tls11 => WreqTlsVersion::TLS_1_1,
            TlsVersion::Tls12 => WreqTlsVersion::TLS_1_2,
            TlsVersion::Tls13 => WreqTlsVersion::TLS_1_3,
        };
        tls = tls
            .min_tls_version(self.min_tls.map(version))
            .max_tls_version(self.max_tls.map(version))
            .grease_enabled(self.grease)
            .preserve_tls13_cipher_list(self.preserve_tls13_cipher_list);
        if let Some(enabled) = self.ocsp_stapling {
            tls = tls.enable_ocsp_stapling(enabled);
        }
        if let Some(enabled) = self.signed_cert_timestamps {
            tls = tls.enable_signed_cert_timestamps(enabled);
        }
        if let Some(list) = &self.cipher_list {
            tls = tls.cipher_list(list.clone());
        }
        if let Some(list) = &self.curves_list {
            tls = tls.curves_list(list.clone());
        }
        if let Some(list) = &self.sigalgs_list {
            tls = tls.sigalgs_list(list.clone());
        }
        builder = builder.tls_options(tls.build());

        if let Some(http2) = &self.http2 {
            let mut options = Http2Options::builder()
                .initial_window_size(http2.initial_window_size)
                .initial_connection_window_size(http2.initial_connection_window_size)
                .max_frame_size(http2.max_frame_size)
                .header_table_size(http2.header_table_size)
                .max_concurrent_streams(http2.max_concurrent_streams);
            if let Some(value) = http2.enable_push {
                options = options.enable_push(value);
            }
            if let Some(value) = http2.max_header_list_size {
                options = options.max_header_list_size(value);
            }
            if let Some(order) = &http2.pseudo_header_order {
                options = options.headers_pseudo_order(
                    PseudoOrder::builder()
                        .extend(order.iter().map(|header| match header {
                            PseudoHeader::Method => PseudoId::Method,
                            PseudoHeader::Scheme => PseudoId::Scheme,
                            PseudoHeader::Authority => PseudoId::Authority,
                            PseudoHeader::Path => PseudoId::Path,
                        }))
                        .build(),
                );
            }
            if let Some(order) = &http2.settings_order {
                options = options.settings_order(
                    SettingsOrder::builder()
                        .extend(order.iter().map(|setting| match setting {
                            Http2Setting::HeaderTableSize => SettingId::HeaderTableSize,
                            Http2Setting::EnablePush => SettingId::EnablePush,
                            Http2Setting::MaxConcurrentStreams => SettingId::MaxConcurrentStreams,
                            Http2Setting::InitialWindowSize => SettingId::InitialWindowSize,
                            Http2Setting::MaxFrameSize => SettingId::MaxFrameSize,
                            Http2Setting::MaxHeaderListSize => SettingId::MaxHeaderListSize,
                        }))
                        .build(),
                );
            }
            if let Some(priority) = http2.headers_priority {
                options = options.headers_stream_dependency(StreamDependency::new(
                    StreamId::from(priority.dependency_id),
                    priority.weight,
                    priority.exclusive,
                ));
            }
            builder = builder.http2_options(options.build());
        }

        if let Some(headers) = &self.headers {
            let mut map = http::HeaderMap::with_capacity(headers.len());
            let mut original = wreq::header::OrigHeaderMap::with_capacity(headers.len());
            for (name, value) in headers {
                let parsed = http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| Error::InvalidConfig("invalid default header name"))?;
                let value = http::HeaderValue::from_str(value)
                    .map_err(|_| Error::InvalidConfig("invalid default header value"))?;
                map.append(parsed, value);
                original.insert(name.clone());
            }
            builder = builder.default_headers(map).orig_headers(original);
        }
        Ok(builder)
    }
}

impl ConnectionConfig {
    /// The portable part of a custom fingerprint. Explicit request headers
    /// retain precedence, just as they do with wreq's default headers.
    pub(crate) fn default_headers(&self) -> Result<http::HeaderMap, Error> {
        let mut headers = http::HeaderMap::new();
        if let Some(EmulationConfig::Custom(fingerprint)) = &self.emulation {
            for (name, value) in fingerprint.headers.iter().flatten() {
                let name = http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| Error::InvalidConfig("invalid emulation header name"))?;
                let value = http::HeaderValue::from_str(value)
                    .map_err(|_| Error::InvalidConfig("invalid emulation header value"))?;
                headers.append(name, value);
            }
        }
        Ok(headers)
    }
}
