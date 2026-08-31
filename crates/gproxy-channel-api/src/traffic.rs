use http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ChannelTrafficPolicy;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "content-length",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];
const REQUEST_DENIED: &[&str] = &[
    "accept-encoding",
    "api-key",
    "authorization",
    "cookie",
    "forwarded",
    "host",
    "via",
    "x-api-key",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-goog-api-key",
    "x-real-ip",
];
const RESPONSE_DENIED: &[&str] = &[
    "alt-svc",
    "server",
    "set-cookie",
    "set-cookie2",
    "via",
    "www-authenticate",
];
const REQUEST_BASE: &[&str] = &["accept", "content-type"];
const RESPONSE_BASE: &[&str] = &[
    "accept-ranges",
    "allow",
    "cache-control",
    "content-disposition",
    "content-encoding",
    "content-range",
    "content-type",
    "etag",
    "expires",
    "last-modified",
    "link",
    "location",
    "retry-after",
    "vary",
];
const QUERY_DENIED: &[&str] = &["access_token", "api_key", "key", "x-api-key"];
const SETTING_KEY: &str = "traffic_policy";
const MAX_ENTRIES: usize = 128;
const MAX_PATTERN_LEN: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficPolicyConfig {
    pub request_headers: Vec<String>,
    pub response_headers: Vec<String>,
    pub request_query: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrafficBlacklistConfig {
    pub request_headers: Vec<String>,
    pub response_headers: Vec<String>,
    pub request_query: Vec<String>,
}

impl TrafficPolicyConfig {
    pub fn configured(settings: &Value) -> Result<Option<Self>, String> {
        let Some(value) = settings.get(SETTING_KEY) else {
            return Ok(None);
        };
        if value.is_null() {
            return Ok(None);
        }
        let config: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("traffic_policy: {error}"))?;
        config.normalized().map(Some)
    }

    pub fn store(settings: &mut Value, config: Option<Self>) -> Result<(), String> {
        let object = settings
            .as_object_mut()
            .ok_or_else(|| "provider settings must be an object".to_owned())?;
        match config {
            Some(config) => {
                object.insert(
                    SETTING_KEY.into(),
                    serde_json::to_value(config.normalized()?)
                        .map_err(|error| error.to_string())?,
                );
            }
            None => {
                object.remove(SETTING_KEY);
            }
        }
        Ok(())
    }

    pub fn remove_from(settings: &mut Value) -> Result<Option<Self>, String> {
        let configured = Self::configured(settings)?;
        if let Some(object) = settings.as_object_mut() {
            object.remove(SETTING_KEY);
        }
        Ok(configured)
    }

    pub fn filter_request_headers(&self, source: &HeaderMap) -> HeaderMap {
        self.filter_request_headers_with(source, &TrafficBlacklistConfig::default())
    }

    pub fn filter_response_headers(&self, source: HeaderMap) -> HeaderMap {
        self.filter_response_headers_with(source, &TrafficBlacklistConfig::default())
    }

    pub fn filter_request_query(&self, query: Option<&str>) -> Option<String> {
        self.filter_request_query_with(query, &TrafficBlacklistConfig::default())
    }

    pub fn filter_request_headers_with(
        &self,
        source: &HeaderMap,
        blacklist: &TrafficBlacklistConfig,
    ) -> HeaderMap {
        request_headers_configured(source, self, blacklist)
    }

    pub fn filter_response_headers_with(
        &self,
        source: HeaderMap,
        blacklist: &TrafficBlacklistConfig,
    ) -> HeaderMap {
        response_headers_configured(source, self, blacklist)
    }

    pub fn filter_request_query_with(
        &self,
        query: Option<&str>,
        blacklist: &TrafficBlacklistConfig,
    ) -> Option<String> {
        request_query_configured(query, self, blacklist)
    }

    fn normalized(mut self) -> Result<Self, String> {
        normalize_patterns(&mut self.request_headers, "request header", true)?;
        normalize_patterns(&mut self.response_headers, "response header", true)?;
        normalize_patterns(&mut self.request_query, "request query", false)?;
        Ok(self)
    }
}

impl TrafficBlacklistConfig {
    pub fn new(
        request_headers: Vec<String>,
        response_headers: Vec<String>,
        request_query: Vec<String>,
    ) -> Result<Self, String> {
        Self {
            request_headers,
            response_headers,
            request_query,
        }
        .normalized()
    }

    pub fn from_value(value: &Value) -> Result<Self, String> {
        serde_json::from_value::<Self>(value.clone())
            .map_err(|error| format!("traffic_blacklist: {error}"))?
            .normalized()
    }

    pub fn defaults() -> Self {
        Self {
            request_headers: merged_owned(HOP_BY_HOP, REQUEST_DENIED),
            response_headers: merged_owned(HOP_BY_HOP, RESPONSE_DENIED),
            request_query: owned(QUERY_DENIED),
        }
    }

    fn normalized(mut self) -> Result<Self, String> {
        normalize_patterns(&mut self.request_headers, "request header blacklist", true)?;
        normalize_patterns(
            &mut self.response_headers,
            "response header blacklist",
            true,
        )?;
        normalize_patterns(&mut self.request_query, "request query blacklist", false)?;
        Ok(self)
    }
}

impl From<ChannelTrafficPolicy> for TrafficPolicyConfig {
    fn from(value: ChannelTrafficPolicy) -> Self {
        Self {
            request_headers: owned(value.request_headers),
            response_headers: owned(value.response_headers),
            request_query: owned(value.request_query),
        }
    }
}

pub fn ingress_headers(source: &HeaderMap) -> HeaderMap {
    filter_headers(source, REQUEST_DENIED, &[], &[], &["*"])
}

pub fn request_headers(source: &HeaderMap, policy: &ChannelTrafficPolicy) -> HeaderMap {
    filter_headers(
        source,
        REQUEST_DENIED,
        &[],
        REQUEST_BASE,
        policy.request_headers,
    )
}

pub fn response_headers(source: HeaderMap, policy: &ChannelTrafficPolicy) -> HeaderMap {
    filter_headers(
        &source,
        RESPONSE_DENIED,
        &[],
        RESPONSE_BASE,
        policy.response_headers,
    )
}

pub fn ingress_query(query: Option<&str>) -> Option<String> {
    filter_query(query, &["*"])
}

pub fn request_query(query: Option<&str>, policy: &ChannelTrafficPolicy) -> Option<String> {
    filter_query(query, policy.request_query)
}

fn request_headers_configured(
    source: &HeaderMap,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> HeaderMap {
    let allowed = borrowed(&policy.request_headers);
    let denied = borrowed(&blacklist.request_headers);
    filter_headers(source, REQUEST_DENIED, &denied, REQUEST_BASE, &allowed)
}

fn response_headers_configured(
    source: HeaderMap,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> HeaderMap {
    let allowed = borrowed(&policy.response_headers);
    let denied = borrowed(&blacklist.response_headers);
    filter_headers(&source, RESPONSE_DENIED, &denied, RESPONSE_BASE, &allowed)
}

fn request_query_configured(
    query: Option<&str>,
    policy: &TrafficPolicyConfig,
    blacklist: &TrafficBlacklistConfig,
) -> Option<String> {
    let allowed = borrowed(&policy.request_query);
    let denied = borrowed(&blacklist.request_query);
    filter_query_with_denied(query, &allowed, &denied)
}

fn filter_headers(
    source: &HeaderMap,
    denied: &[&str],
    extra_denied: &[&str],
    base: &[&str],
    allowed: &[&str],
) -> HeaderMap {
    let nominated = connection_nominated(source);
    let mut output = HeaderMap::with_capacity(source.len());
    for (name, value) in source {
        let name_str = name.as_str();
        if !HOP_BY_HOP.contains(&name_str)
            && !denied.contains(&name_str)
            && !matches_name(name_str, extra_denied)
            && !nominated.iter().any(|candidate| candidate == name_str)
            && (base.contains(&name_str) || matches_name(name_str, allowed))
        {
            output.append(name.clone(), value.clone());
        }
    }
    output
}

fn filter_query(query: Option<&str>, allowed: &[&str]) -> Option<String> {
    filter_query_with_denied(query, allowed, &[])
}

fn filter_query_with_denied(
    query: Option<&str>,
    allowed: &[&str],
    extra_denied: &[&str],
) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            !name.is_empty()
                && !QUERY_DENIED.contains(&name.to_ascii_lowercase().as_str())
                && !matches_name(name, extra_denied)
                && matches_name(name, allowed)
        })
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

fn matches_name(name: &str, allowed: &[&str]) -> bool {
    allowed.iter().any(|pattern| {
        *pattern == "*"
            || pattern
                .strip_suffix('*')
                .is_some_and(|prefix| name.starts_with(prefix))
            || *pattern == name
    })
}

fn connection_nominated(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(http::header::CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect()
}

fn normalize_patterns(patterns: &mut Vec<String>, label: &str, header: bool) -> Result<(), String> {
    if patterns.len() > MAX_ENTRIES {
        return Err(format!("{label} allow-list exceeds {MAX_ENTRIES} entries"));
    }
    for pattern in patterns.iter_mut() {
        *pattern = pattern.trim().to_owned();
        if pattern.is_empty() || pattern.len() > MAX_PATTERN_LEN {
            return Err(format!("invalid {label} pattern"));
        }
        if header {
            *pattern = pattern.to_ascii_lowercase();
            let name = pattern.strip_suffix('*').unwrap_or(pattern);
            if pattern != "*"
                && (name.is_empty() || http::HeaderName::from_bytes(name.as_bytes()).is_err())
            {
                return Err(format!("invalid {label} pattern `{pattern}`"));
            }
        } else if pattern != "*"
            && pattern
                .bytes()
                .any(|byte| byte.is_ascii_control() || matches!(byte, b'&' | b'=' | b'#' | b'?'))
        {
            return Err(format!("invalid {label} pattern `{pattern}`"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    patterns.retain(|pattern| seen.insert(pattern.clone()));
    Ok(())
}

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn borrowed(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

fn merged_owned(left: &[&str], right: &[&str]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    left.iter()
        .chain(right)
        .filter(|value| seen.insert(**value))
        .map(|value| (*value).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: ChannelTrafficPolicy =
        ChannelTrafficPolicy::new(&["x-safe-*"], &["x-result-*"], &["*"]);

    #[test]
    fn global_denials_override_channel_wildcards() {
        let mut headers = HeaderMap::new();
        headers.insert("x-safe-id", "one".parse().unwrap());
        headers.insert("x-api-key", "secret".parse().unwrap());
        let filtered = request_headers(&headers, &POLICY);
        assert!(filtered.contains_key("x-safe-id"));
        assert!(!filtered.contains_key("x-api-key"));
        assert_eq!(
            request_query(Some("page=2&key=secret"), &POLICY).as_deref(),
            Some("page=2")
        );
    }

    #[test]
    fn response_filter_preserves_repeated_allowed_values_without_cookies() {
        let mut headers = HeaderMap::new();
        headers.append("x-result-id", "one".parse().unwrap());
        headers.append("x-result-id", "two".parse().unwrap());
        headers.insert("set-cookie", "session=secret".parse().unwrap());
        let filtered = response_headers(headers, &POLICY);
        assert_eq!(filtered.get_all("x-result-id").iter().count(), 2);
        assert!(!filtered.contains_key("set-cookie"));
    }

    #[test]
    fn configured_policy_is_normalized_stored_and_removed() {
        let mut settings = serde_json::json!({});
        TrafficPolicyConfig::store(
            &mut settings,
            Some(TrafficPolicyConfig {
                request_headers: vec![" X-Custom-* ".into(), "x-custom-*".into()],
                response_headers: vec!["X-Result".into()],
                request_query: vec!["pageToken".into()],
            }),
        )
        .unwrap();
        let configured = TrafficPolicyConfig::configured(&settings).unwrap().unwrap();
        assert_eq!(configured.request_headers, ["x-custom-*"]);
        assert_eq!(configured.response_headers, ["x-result"]);
        assert_eq!(configured.request_query, ["pageToken"]);
        assert_eq!(
            TrafficPolicyConfig::remove_from(&mut settings).unwrap(),
            Some(configured)
        );
        assert_eq!(settings, serde_json::json!({}));
    }

    #[test]
    fn malformed_policy_patterns_are_rejected_at_the_settings_boundary() {
        let settings = serde_json::json!({
            "traffic_policy": {
                "request_headers": ["bad header"],
                "response_headers": [],
                "request_query": []
            }
        });
        assert!(TrafficPolicyConfig::configured(&settings).is_err());
    }
}
