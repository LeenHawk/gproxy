use gproxy_protocol::OperationKey;

/// One declared route through a channel: the client's wire shape and the
/// native wire shape the channel receives after any transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelSupport {
    pub source: OperationKey,
    pub target: OperationKey,
    pub action: ChannelRouteAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelRouteAction {
    Passthrough,
    TransformTo,
    Local,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelFieldControl {
    Text,
    Secret,
    Url,
    Integer,
    Boolean,
    StringList,
    Select,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelField {
    pub key: &'static str,
    pub i18n_key: &'static str,
    pub control: ChannelFieldControl,
    pub required: bool,
    pub advanced: bool,
    pub default_value: Option<&'static str>,
    pub options: &'static [&'static str],
}

impl ChannelSupport {
    pub const fn passthrough(key: OperationKey) -> Self {
        Self {
            source: key,
            target: key,
            action: ChannelRouteAction::Passthrough,
        }
    }

    pub const fn transform(source: OperationKey, target: OperationKey) -> Self {
        Self {
            source,
            target,
            action: ChannelRouteAction::TransformTo,
        }
    }

    pub const fn local(source: OperationKey) -> Self {
        Self {
            source,
            target: source,
            action: ChannelRouteAction::Local,
        }
    }

    pub const fn unsupported(source: OperationKey) -> Self {
        Self {
            source,
            target: source,
            action: ChannelRouteAction::Unsupported,
        }
    }
}

/// Identity and capability card. Routes are not here: they are declared once
/// in [`crate::Channel::routing_table`] and read through
/// [`crate::executable_routes`].
#[derive(Debug)]
pub struct ChannelDescriptor {
    /// Stable id: `"openai"`, `"claudecode"`, `"codex"`.
    pub id: &'static str,
    pub display_name: &'static str,
    pub provider_fields: &'static [ChannelField],
    pub credential_fields: &'static [ChannelField],
    pub endpoint_overrides: bool,
    pub traffic_policy: ChannelTrafficPolicy,
}

/// Caller-controlled metadata a channel permits across the gateway boundary.
/// The core adds its universal HTTP allow-list and always applies its global
/// credential/hop-by-hop deny-list first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelTrafficPolicy {
    pub request_headers: &'static [&'static str],
    pub response_headers: &'static [&'static str],
    pub request_query: &'static [&'static str],
}

impl ChannelTrafficPolicy {
    pub const fn new(
        request_headers: &'static [&'static str],
        response_headers: &'static [&'static str],
        request_query: &'static [&'static str],
    ) -> Self {
        Self {
            request_headers,
            response_headers,
            request_query,
        }
    }

    pub fn effective_traffic_policy(
        &self,
        settings: &serde_json::Value,
    ) -> Result<crate::TrafficPolicyConfig, String> {
        Ok(crate::TrafficPolicyConfig::configured(settings)?
            .unwrap_or_else(|| crate::TrafficPolicyConfig::from(*self)))
    }

    pub fn filter_request_headers(
        &self,
        source: &http::HeaderMap,
        settings: &serde_json::Value,
    ) -> Result<http::HeaderMap, String> {
        Ok(self
            .effective_traffic_policy(settings)?
            .filter_request_headers(source))
    }

    pub fn filter_request_query(
        &self,
        query: Option<&str>,
        settings: &serde_json::Value,
    ) -> Result<Option<String>, String> {
        Ok(self
            .effective_traffic_policy(settings)?
            .filter_request_query(query))
    }
}
