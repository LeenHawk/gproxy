//! Host authorisation for `ResourceReference::Url` reads. Core fetches a URL
//! only after the policy allowed it, and asks again on every redirect hop.
//!
//! The policy sees the URL and the addresses the runtime resolver returned
//! for its host (`resolved`). Natively that is the tokio resolver's answer; on
//! wasm32 there is no resolver and the slice is empty, so a policy decides on
//! the hostname alone (`DefaultFetchPolicy` then still denies literal IP hosts
//! in the blocked ranges and `localhost`). Core cannot pin the connection to
//! the addresses it checked: the outbound client resolves again when it
//! connects, so a name that rebinds between the two lookups reaches whatever
//! it points at then. Hosts that must close that window front the fetch with
//! an egress proxy or an allow-list of names they control.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The answer to one URL, given for the initial request and each redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchDecision {
    Allow,
    /// Refused; the reason ends up in the `Unsupported` error's message.
    Deny(&'static str),
}

/// Which URLs core may fetch on the caller's behalf. Set through
/// `CoreBuilder::fetch_policy`; `DefaultFetchPolicy` when unset.
pub trait FetchPolicy: Send + Sync {
    fn decide(&self, url: &url::Url, resolved: &[IpAddr]) -> FetchDecision;
}

/// http/https to public addresses only. Denies loopback, link-local,
/// private (RFC 1918 and fc00::/7), unspecified, broadcast and multicast
/// addresses, checking IPv4-mapped IPv6 forms as IPv4, whether the address
/// is the URL's literal host or the resolver's answer for its name.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultFetchPolicy;

/// Every http/https URL, no address checks. The right choice for a
/// single-user host whose own network is the caller's network.
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAllFetchPolicy;

/// http/https URLs whose host matches one entry: an exact name, or
/// `*.suffix` for any name below `suffix` (not `suffix` itself). Names
/// compare case-insensitively; addresses are not checked, the operator
/// vouches for the listed hosts.
#[derive(Debug, Clone, Default)]
pub struct AllowlistFetchPolicy {
    hosts: Vec<String>,
}

impl AllowlistFetchPolicy {
    pub fn new(hosts: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            hosts: hosts
                .into_iter()
                .map(|host| {
                    host.into()
                        .trim()
                        .trim_end_matches('.')
                        .to_ascii_lowercase()
                })
                .filter(|host| !host.is_empty())
                .collect(),
        }
    }

    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }

    fn matches(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts
            .iter()
            .any(|entry| match entry.strip_prefix("*.") {
                Some(suffix) => host
                    .strip_suffix(suffix)
                    .is_some_and(|prefix| prefix.ends_with('.') && prefix.len() > 1),
                None => *entry == host,
            })
    }
}

fn web_scheme(url: &url::Url) -> Result<(), &'static str> {
    match url.scheme() {
        "http" | "https" => Ok(()),
        _ => Err("only http and https URLs are fetched"),
    }
}

fn blocked_v4(ip: Ipv4Addr) -> Option<&'static str> {
    if ip.is_loopback() {
        Some("loopback address")
    } else if ip.is_private() {
        Some("private address")
    } else if ip.is_link_local() {
        Some("link-local address")
    } else if ip.is_unspecified() {
        Some("unspecified address")
    } else if ip.is_broadcast() {
        Some("broadcast address")
    } else if ip.is_multicast() {
        Some("multicast address")
    } else {
        None
    }
}

fn blocked_v6(ip: Ipv6Addr) -> Option<&'static str> {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return blocked_v4(v4);
    }
    if ip.is_loopback() {
        Some("loopback address")
    } else if ip.is_unique_local() {
        Some("private address")
    } else if ip.is_unicast_link_local() {
        Some("link-local address")
    } else if ip.is_unspecified() {
        Some("unspecified address")
    } else if ip.is_multicast() {
        Some("multicast address")
    } else {
        None
    }
}

/// Why `ip` is not a public address, or `None` when it is.
pub fn blocked_address(ip: IpAddr) -> Option<&'static str> {
    match ip {
        IpAddr::V4(ip) => blocked_v4(ip),
        IpAddr::V6(ip) => blocked_v6(ip),
    }
}

impl FetchPolicy for DefaultFetchPolicy {
    fn decide(&self, url: &url::Url, resolved: &[IpAddr]) -> FetchDecision {
        if let Err(reason) = web_scheme(url) {
            return FetchDecision::Deny(reason);
        }
        let literal = match url.host() {
            None => return FetchDecision::Deny("URL has no host"),
            Some(url::Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
            Some(url::Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
            Some(url::Host::Domain(name)) => {
                let name = name.trim_end_matches('.');
                if name.eq_ignore_ascii_case("localhost")
                    || name
                        .rsplit_once('.')
                        .is_some_and(|(_, tld)| tld.eq_ignore_ascii_case("localhost"))
                {
                    return FetchDecision::Deny("loopback name");
                }
                None
            }
        };
        for ip in literal.iter().chain(resolved) {
            if let Some(reason) = blocked_address(*ip) {
                return FetchDecision::Deny(reason);
            }
        }
        FetchDecision::Allow
    }
}

impl FetchPolicy for AllowAllFetchPolicy {
    fn decide(&self, url: &url::Url, _: &[IpAddr]) -> FetchDecision {
        match web_scheme(url) {
            Ok(()) => FetchDecision::Allow,
            Err(reason) => FetchDecision::Deny(reason),
        }
    }
}

impl FetchPolicy for AllowlistFetchPolicy {
    fn decide(&self, url: &url::Url, _: &[IpAddr]) -> FetchDecision {
        if let Err(reason) = web_scheme(url) {
            return FetchDecision::Deny(reason);
        }
        match url.host_str() {
            Some(host) if self.matches(host) => FetchDecision::Allow,
            Some(_) => FetchDecision::Deny("host is not on the fetch allow-list"),
            None => FetchDecision::Deny("URL has no host"),
        }
    }
}
