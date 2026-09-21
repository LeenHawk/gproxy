//! Which client headers reach the upstream. Every channel drops hop-by-hop
//! headers, the source host/length and any source authentication; a provider
//! may additionally restrict forwarding to an allow-list so only the headers
//! it names leave the gateway. Headers a channel injects itself (its own
//! authentication, static config headers) are not subject to the list.

use super::{ChannelError, ProviderView};
use http::{HeaderMap, HeaderName};

/// Headers a vendor's own client sends that its channel always lets through,
/// even under a provider allow-list: the list narrows what *other* clients
/// may add, it must not strip the CLI the channel impersonates. Names are
/// exact lowercase header names; prefixes match any header starting with
/// them (`x-codex-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChannelHeaders {
    pub names: &'static [&'static str],
    pub prefixes: &'static [&'static str],
}

impl ChannelHeaders {
    pub const NONE: Self = Self {
        names: &[],
        prefixes: &[],
    };

    fn allows(&self, name: &HeaderName) -> bool {
        let name = name.as_str();
        self.names.contains(&name) || self.prefixes.iter().any(|prefix| name.starts_with(prefix))
    }
}

/// Provider `config.allowed_headers`: the only client headers forwarded.
/// `content-type` is always forwarded because it describes the body being
/// sent, not the client; a channel's own `ChannelHeaders` are forwarded too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAllowlist {
    names: Vec<HeaderName>,
    channel: ChannelHeaders,
}

impl HeaderAllowlist {
    /// None when the provider configures no list (forward everything that is
    /// not dropped); an invalid header name is a configuration error.
    pub fn from_view(provider: ProviderView<'_>) -> Result<Option<Self>, ChannelError> {
        Self::from_view_for(provider, ChannelHeaders::NONE)
    }

    /// Like `from_view`, with the headers the channel's vendor client sends
    /// always allowed.
    pub fn from_view_for(
        provider: ProviderView<'_>,
        channel: ChannelHeaders,
    ) -> Result<Option<Self>, ChannelError> {
        let Some(value) = provider.config.get("allowed_headers") else {
            return Ok(None);
        };
        let names = value
            .as_array()
            .ok_or_else(|| ChannelError::InvalidConfig("allowed_headers must be an array".into()))?
            .iter()
            .map(|item| {
                item.as_str()
                    .and_then(|name| HeaderName::from_bytes(name.trim().as_bytes()).ok())
                    .ok_or_else(|| {
                        ChannelError::InvalidConfig(format!(
                            "allowed_headers entry `{item}` is not a header name"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(Self { names, channel }))
    }

    pub fn allows(&self, name: &HeaderName) -> bool {
        name == http::header::CONTENT_TYPE || self.channel.allows(name) || self.names.contains(name)
    }
}

/// Headers never forwarded by any channel.
const ALWAYS_DROP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "api-key",
];

/// Copy the forwardable client headers, keeping repeated values and order.
/// `also_drop` names the channel's own authentication and identity headers so
/// a client cannot spoof them.
pub fn forwardable(
    source: &HeaderMap,
    allowlist: Option<&HeaderAllowlist>,
    also_drop: &[&str],
) -> HeaderMap {
    let mut out = HeaderMap::with_capacity(source.len());
    for (name, value) in source {
        if ALWAYS_DROP.contains(&name.as_str()) || also_drop.contains(&name.as_str()) {
            continue;
        }
        if allowlist.is_some_and(|list| !list.allows(name)) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}
