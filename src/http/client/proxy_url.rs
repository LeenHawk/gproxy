//! Proxy URL normalization applied before handing the URL to wreq.
//!
//! Many residential-proxy vendors hand out credentials in the colon-separated
//! form `scheme://host:port:user:pass`, which is not a valid URL (wreq rejects
//! it → "invalid proxy"). This module rewrites that form to the standard
//! `scheme://user:pass@host:port`, percent-encoding the userinfo. Anything
//! ambiguous (already has `@`, not exactly 4 segments, non-numeric port) is
//! passed through untouched for wreq to judge.

use std::borrow::Cow;

/// Normalize a proxy URL, rewriting the vendor colon-form
/// `scheme://host:port:user:pass` to `scheme://user:pass@host:port`.
pub fn normalize_proxy_url(raw: &str) -> Cow<'_, str> {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Cow::Borrowed(raw);
    };
    // Standard form (credentials via userinfo, or none) — leave untouched.
    if rest.contains('@') {
        return Cow::Borrowed(raw);
    }
    let parts: Vec<&str> = rest.split(':').collect();
    let [host, port, user, pass] = parts[..] else {
        return Cow::Borrowed(raw);
    };
    // Require a plausible host:port head; otherwise (IPv6 literal, stray
    // colons…) the split is ambiguous — pass through unchanged.
    if host.is_empty() || user.is_empty() || port.parse::<u16>().is_err() {
        return Cow::Borrowed(raw);
    }
    Cow::Owned(format!(
        "{scheme}://{}:{}@{host}:{port}",
        encode_userinfo(user),
        encode_userinfo(pass),
    ))
}

/// Percent-encode one userinfo segment (user or password) per RFC 3986:
/// keep unreserved characters, escape everything else (incl. `:` / `@`).
fn encode_userinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::normalize_proxy_url;

    /// Regression: vendor colon-form must be rewritten to standard userinfo
    /// form so wreq accepts it, while standard/ambiguous inputs pass through.
    #[test]
    fn colon_form_is_rewritten_and_others_pass_through() {
        assert_eq!(
            normalize_proxy_url("socks5://sg.example.io:3010:user-region-US-sid-x:pw"),
            "socks5://user-region-US-sid-x:pw@sg.example.io:3010"
        );
        // Special chars in credentials get percent-encoded.
        assert_eq!(
            normalize_proxy_url("http://h:80:u:p@w"),
            "http://h:80:u:p@w" // already contains '@' → untouched
        );
        assert_eq!(
            normalize_proxy_url("http://h:80:u/x:p w"),
            "http://u%2Fx:p%20w@h:80"
        );
        // Standard forms and ambiguous inputs are untouched.
        for raw in [
            "socks5://user:pass@host:1080",
            "http://127.0.0.1:7890",
            "socks5://h:80:u:p:extra", // 5 segments
            "socks5://h:notaport:u:p", // non-numeric port
            "host:3010:user:pass",     // no scheme
        ] {
            assert_eq!(normalize_proxy_url(raw), raw);
        }
    }
}
