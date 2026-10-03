//! Host authorisation for `ResourceReference::Url` reads. Core fetches a URL
//! only after the policy allowed it, and asks again on every redirect hop.
//!
//! The policy sees the URL and the addresses the runtime resolver returned
//! for its host (`resolved`). Natively that is the tokio resolver's answer; on
//! wasm32 there is no resolver and the slice is empty, so a policy decides on
//! the hostname alone (`DefaultFetchPolicy` then still denies literal IP hosts
//! in the blocked ranges and `localhost`).
//!
//! Where the vetted answer binds the connection depends on the transport:
//!
//! * Native, direct: the fetch connects only to the addresses the policy saw
//!   (a pinned client), so a name that rebinds between lookup and connect
//!   cannot reach an address the policy never judged.
//! * Native, through the configured proxy: the name is still resolved here
//!   and vetted, and a name that fails to resolve is refused, but the proxy
//!   resolves it again and makes the connection. This process has no
//!   connection to pin, so a rebinding name, or a name the proxy resolves
//!   differently (split-horizon DNS), reaches whatever the proxy reaches.
//! * wasm32: no resolver, no pinning. Only literal IP hosts and loopback
//!   names are judged; any other name is fetched wherever the runtime's
//!   `fetch` sends it.
//!
//! In the last two cases the destination policy is the proxy's or the
//! platform egress's to enforce: a deployment that fetches untrusted links
//! through a proxy or on an edge runtime must have that egress refuse
//! internal destinations itself, or use an allow-list of names it controls.

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

/// http/https to globally routable addresses only. Denies every range the
/// IANA IPv4 and IPv6 special-purpose registries mark as not globally
/// reachable (loopback, private, shared/CGNAT, link-local, documentation,
/// benchmarking, protocol assignments, discard, site-local, NAT64
/// local-use), multicast, broadcast, reserved space and IPv6 outside
/// 2000::/3. IPv4-mapped, NAT64 (64:ff9b::/96) and 6to4 forms are judged by
/// the IPv4 address they embed. The check applies whether the address is the
/// URL's literal host or the resolver's answer for its name; see
/// [`blocked_address`] for the exact table. Hosts that must reach internal
/// targets choose [`AllowlistFetchPolicy`] or [`AllowAllFetchPolicy`].
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

/// Whether `ip` lies in `prefix/len`, comparing the top `len` bits.
fn in_v4(ip: Ipv4Addr, prefix: [u8; 4], len: u32) -> bool {
    let mask = u32::MAX.checked_shl(32 - len).unwrap_or(0);
    u32::from(ip) & mask == u32::from(Ipv4Addr::from(prefix)) & mask
}

/// Whether `ip` lies in `prefix/len`, comparing the top `len` bits.
fn in_v6(ip: Ipv6Addr, prefix: u128, len: u32) -> bool {
    let mask = u128::MAX.checked_shl(128 - len).unwrap_or(0);
    u128::from(ip) & mask == prefix & mask
}

/// The IPv4 address carried in the low 32 bits of `ip` (NAT64 well-known
/// prefix and IPv4-mapped forms put it there).
fn low_v4(ip: Ipv6Addr) -> Ipv4Addr {
    Ipv4Addr::from(u128::from(ip) as u32)
}

/// Every IPv4 range the IANA IPv4 Special-Purpose Address Registry marks as
/// not globally reachable, plus the multicast and reserved blocks (224/4,
/// 240/4) that are never a unicast fetch target. Ordered so the specific
/// reasons win over the enclosing block (`0.0.0.0` before `0/8`,
/// `255.255.255.255` before `240/4`). The registry lists a few /32s inside
/// 192.0.0.0/24 as globally reachable anycast (PCP, TURN); the whole /24 is
/// still denied, nothing a media link points at lives there.
fn blocked_v4(ip: Ipv4Addr) -> Option<&'static str> {
    const RANGES: &[([u8; 4], u32, &str)] = &[
        ([0, 0, 0, 0], 32, "unspecified address"),
        ([0, 0, 0, 0], 8, "this-network address"),
        ([10, 0, 0, 0], 8, "private address"),
        ([100, 64, 0, 0], 10, "shared address space"),
        ([127, 0, 0, 0], 8, "loopback address"),
        ([169, 254, 0, 0], 16, "link-local address"),
        ([172, 16, 0, 0], 12, "private address"),
        ([192, 0, 0, 0], 24, "IETF protocol assignment"),
        ([192, 0, 2, 0], 24, "documentation address"),
        ([192, 88, 99, 0], 24, "6to4 relay anycast address"),
        ([192, 168, 0, 0], 16, "private address"),
        ([198, 18, 0, 0], 15, "benchmarking address"),
        ([198, 51, 100, 0], 24, "documentation address"),
        ([203, 0, 113, 0], 24, "documentation address"),
        ([224, 0, 0, 0], 4, "multicast address"),
        ([255, 255, 255, 255], 32, "broadcast address"),
        ([240, 0, 0, 0], 4, "reserved address"),
    ];
    RANGES
        .iter()
        .find(|(prefix, len, _)| in_v4(ip, *prefix, *len))
        .map(|(_, _, reason)| *reason)
}

/// Every IPv6 range the IANA IPv6 Special-Purpose Address Registry marks as
/// not globally reachable, and everything outside 2000::/3, the only block
/// IANA allocates as global unicast (the rest of the space is reserved,
/// unique-local, link-local, site-local or multicast; SRv6 SIDs in 5f00::/16
/// fall here too). Forms that embed an IPv4 address a router would deliver
/// to (IPv4-mapped, NAT64 well-known prefix 64:ff9b::/96, 6to4 2002::/16)
/// are judged by that IPv4 address, so `::ffff:127.0.0.1` and
/// `2002:7f00:1::` are loopback while `64:ff9b::93.184.216.34` is public.
/// Teredo (2001::/32) is denied outright with the rest of 2001::/23: the
/// mapped client address is obfuscated and the relay is not ours to vet.
fn blocked_v6(ip: Ipv6Addr) -> Option<&'static str> {
    if ip.is_unspecified() {
        return Some("unspecified address");
    }
    if ip.is_loopback() {
        return Some("loopback address");
    }
    if let Some(v4) = ip.to_ipv4_mapped() {
        return blocked_v4(v4);
    }
    if in_v6(ip, 0x0064_ff9b << 96, 96) {
        return blocked_v4(low_v4(ip));
    }
    if in_v6(ip, 0x2002 << 112, 16) {
        return blocked_v4(Ipv4Addr::from((u128::from(ip) >> 80) as u32));
    }
    const RANGES: &[(u128, u32, &str)] = &[
        (0x0064_ff9b_0001 << 80, 48, "NAT64 local-use address"),
        (0x0100 << 112, 64, "discard-only address"),
        (0x2001_0db8 << 96, 32, "documentation address"),
        (0x2001 << 112, 23, "IETF protocol assignment"),
        (0x3fff << 112, 20, "documentation address"),
        (0xfc00 << 112, 7, "private address"),
        (0xfe80 << 112, 10, "link-local address"),
        (0xfec0 << 112, 10, "site-local address"),
        (0xff00 << 112, 8, "multicast address"),
    ];
    if let Some((_, _, reason)) = RANGES
        .iter()
        .find(|(prefix, len, _)| in_v6(ip, *prefix, *len))
    {
        return Some(reason);
    }
    if !in_v6(ip, 0x2000 << 112, 3) {
        return Some("reserved address");
    }
    None
}

/// Why `ip` is not a globally routable address, or `None` when it is. The
/// table is `blocked_v4` / `blocked_v6`: the IANA special-purpose
/// registries plus multicast, reserved space and IPv6 outside 2000::/3.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(raw: &str) -> Option<&'static str> {
        blocked_address(raw.parse().expect("test address"))
    }

    #[test]
    fn special_purpose_ipv4_ranges_are_blocked() {
        for (raw, expected) in [
            ("0.0.0.0", "unspecified address"),
            ("0.1.2.3", "this-network address"),
            ("10.255.0.1", "private address"),
            ("100.64.0.1", "shared address space"),
            ("100.127.255.254", "shared address space"),
            ("127.0.0.1", "loopback address"),
            ("127.255.255.254", "loopback address"),
            ("169.254.169.254", "link-local address"),
            ("172.16.0.1", "private address"),
            ("172.31.255.255", "private address"),
            ("192.0.0.8", "IETF protocol assignment"),
            ("192.0.2.1", "documentation address"),
            ("192.88.99.1", "6to4 relay anycast address"),
            ("192.168.1.1", "private address"),
            ("198.18.0.1", "benchmarking address"),
            ("198.19.255.255", "benchmarking address"),
            ("198.51.100.7", "documentation address"),
            ("203.0.113.9", "documentation address"),
            ("224.0.0.1", "multicast address"),
            ("239.255.255.250", "multicast address"),
            ("240.0.0.1", "reserved address"),
            ("255.255.255.254", "reserved address"),
            ("255.255.255.255", "broadcast address"),
        ] {
            assert_eq!(reason(raw), Some(expected), "{raw}");
        }
    }

    #[test]
    fn public_ipv4_neighbours_of_blocked_ranges_are_allowed() {
        for raw in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "100.63.255.255",
            "100.128.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "192.0.1.1",
            "192.0.3.0",
            "192.88.98.255",
            "198.17.255.255",
            "198.20.0.0",
            "198.51.101.0",
            "203.0.114.0",
            "223.255.255.255",
        ] {
            assert_eq!(reason(raw), None, "{raw}");
        }
    }

    #[test]
    fn special_purpose_ipv6_ranges_are_blocked() {
        for (raw, expected) in [
            ("::", "unspecified address"),
            ("::1", "loopback address"),
            ("::ffff:127.0.0.1", "loopback address"),
            ("::ffff:100.64.0.1", "shared address space"),
            ("::7f00:1", "reserved address"),
            ("64:ff9b::127.0.0.1", "loopback address"),
            ("64:ff9b::10.0.0.1", "private address"),
            ("64:ff9b:1::1", "NAT64 local-use address"),
            ("100::1", "discard-only address"),
            ("2001::1", "IETF protocol assignment"),
            (
                "2001:0:4136:e378:8000:63bf:3fff:fdd2",
                "IETF protocol assignment",
            ),
            ("2001:2::1", "IETF protocol assignment"),
            ("2001:1ff::1", "IETF protocol assignment"),
            ("2001:db8::1", "documentation address"),
            ("2002:7f00:1::", "loopback address"),
            ("2002:a9fe:a9fe::1", "link-local address"),
            ("2002:c0a8:101::1", "private address"),
            ("3fff::1", "documentation address"),
            ("3fff:fff::1", "documentation address"),
            ("fc00::1", "private address"),
            ("fd12:3456::1", "private address"),
            ("fe80::1", "link-local address"),
            ("febf::1", "link-local address"),
            ("fec0::1", "site-local address"),
            ("ff02::1", "multicast address"),
            ("5f00::1", "reserved address"),
            ("4000::1", "reserved address"),
        ] {
            assert_eq!(reason(raw), Some(expected), "{raw}");
        }
    }

    #[test]
    fn public_ipv6_and_embedded_public_ipv4_are_allowed() {
        for raw in [
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
            "2001:200::1",
            "2001:db9::1",
            "3fff:1000::1",
            "::ffff:93.184.216.34",
            "64:ff9b::93.184.216.34",
            "2002:5db8:d822::1",
        ] {
            assert_eq!(reason(raw), None, "{raw}");
        }
    }

    #[test]
    fn default_policy_denies_special_purpose_literals_and_answers() {
        let policy = DefaultFetchPolicy;
        let url = |raw: &str| url::Url::parse(raw).expect("test url");
        assert_eq!(
            policy.decide(&url("http://100.100.100.100/x"), &[]),
            FetchDecision::Deny("shared address space")
        );
        assert_eq!(
            policy.decide(&url("http://[64:ff9b::a9fe:a9fe]/latest"), &[]),
            FetchDecision::Deny("link-local address")
        );
        let public: IpAddr = "93.184.216.34".parse().unwrap();
        let cgnat: IpAddr = "100.64.1.1".parse().unwrap();
        assert_eq!(
            policy.decide(&url("https://files.example/a"), &[public, cgnat]),
            FetchDecision::Deny("shared address space")
        );
        assert_eq!(
            policy.decide(&url("https://files.example/a"), &[public]),
            FetchDecision::Allow
        );
    }
}
