//! Provider execution data and prepared rewrite matchers. The upper layer owns
//! routing, caller policy and assembly of the permitted execution target.

use crate::{limits::ExecutionLimits, runtime::CredentialState};
use gproxy_channel::{BaseChannel, OutboundClient, channel::QuotaDimension};
use gproxy_protocol::OperationKey;
use gproxy_store::entity::upstream;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub type EntityMap<M> = HashMap<String, Arc<M>>;
pub use upstream::credential::CredentialStatus;
pub use upstream::operation_endpoint::EndpointTransport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OperationEndpointKey {
    pub operation: OperationKey,
    pub transport: EndpointTransport,
}

/// Instance-wide durable configuration revision, not a per-profile version.
/// Persistence and transactional revision allocation remain to be implemented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConfigRevision(pub u64);

/// No Debug/Serialize: provider configuration may contain sensitive data.
/// Routing, model aliases, identity and business policies belong to the upper layer.
#[derive(Default)]
pub struct CoreData {
    pub revision: ConfigRevision,
    pub observation: crate::ObservationSettings,
    /// Finite execution bounds from the Setting row of this revision.
    pub limits: ExecutionLimits,
    pub providers: EntityMap<ProviderData>,
    pub credentials: EntityMap<CredentialData>,
    pub rewrite_rule_sets: EntityMap<RewriteRuleSetData>,
    /// Local token estimation for exchanges without reported usage; None when
    /// the Setting row disables usage.
    pub estimation: Option<Arc<crate::estimate::Estimator>>,
    pub local_tokenizer: Arc<crate::estimate::Estimator>,
    /// Enabled caller budgets (`quotas` rows with metric `cost`), resolved
    /// per request from the owners the host names.
    pub budgets: Vec<Arc<crate::budget::BudgetData>>,
    /// Enabled operator limits on credentials (`quotas` rows owned by a
    /// `credential` or `provider`); each covered credential also carries
    /// its own in `CredentialData::limits`.
    pub credential_limits: Vec<Arc<crate::credential_limit::CredentialLimit>>,
    /// Enabled price rules with their rates and tiers; every exchange is
    /// priced from here at settlement.
    pub pricing: Arc<crate::pricing::PriceBook>,
}

pub struct ProviderData {
    pub entity: Arc<upstream::provider::Model>,
    /// Runtime config with the global and provider header allow-lists merged.
    /// The persisted entity keeps the provider's own settings unchanged.
    pub effective_config: serde_json::Value,
    pub channel: Arc<dyn BaseChannel>,
    /// References CoreData.credentials; no duplicate credential ownership.
    pub credential_ids: Vec<String>,
    pub models: Vec<Arc<upstream::provider_model::Model>>,
    pub operation_rules: Vec<Arc<upstream::operation_rule::Model>>,
    /// Enabled, validated per-method URLs keyed by the destination OperationKey
    /// and transport. Absent entries use provider/channel URL defaults.
    pub operation_urls: HashMap<OperationEndpointKey, String>,
    /// Ordered by sort_order then ID; shared compiled sets live in CoreData.
    pub rewrite_rule_sets: Vec<Arc<upstream::provider_rewrite_rule_set::Model>>,
    /// Read from the provider config JSON field `credential_strategy`; absent
    /// means round-robin.
    pub credential_strategy: CredentialStrategy,
    /// Independent affinity switch; legacy sticky strategies default to true.
    pub session_affinity: bool,
}

impl ProviderData {
    pub fn view(&self) -> gproxy_channel::channel::ProviderView<'_> {
        gproxy_channel::channel::ProviderView {
            config: &self.effective_config,
            ..crate::assemble::provider_view(&self.entity)
        }
    }

    pub fn operation_url(
        &self,
        operation: OperationKey,
        transport: EndpointTransport,
    ) -> Option<&str> {
        self.operation_urls
            .get(&OperationEndpointKey {
                operation,
                transport,
            })
            .map(String::as_str)
    }

    /// The configured URL with `{model}` filled in with the upstream model,
    /// percent-encoded as one path segment. A template needs the model in the
    /// path for surfaces that put it there (Gemini's `models/{model}:…`, a
    /// relay keyed by model); without a model the URL is used as written.
    pub fn operation_url_for(
        &self,
        operation: OperationKey,
        transport: EndpointTransport,
        model: Option<&str>,
    ) -> Option<std::borrow::Cow<'_, str>> {
        Some(fill_model(self.operation_url(operation, transport)?, model))
    }
}

fn fill_model<'a>(url: &'a str, model: Option<&str>) -> std::borrow::Cow<'a, str> {
    match model.filter(|_| url.contains(MODEL_PLACEHOLDER)) {
        Some(model) => std::borrow::Cow::Owned(url.replace(
            MODEL_PLACEHOLDER,
            &percent_encoding::utf8_percent_encode(model, MODEL_SEGMENT).to_string(),
        )),
        None => std::borrow::Cow::Borrowed(url),
    }
}

/// The placeholder an operation URL may carry for the upstream model.
pub const MODEL_PLACEHOLDER: &str = "{model}";

/// Everything but RFC 3986 unreserved characters, so a model name stays one
/// path segment.
const MODEL_SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStrategy {
    /// Rotate on each selection, without a session pin.
    #[default]
    RoundRobin,
    /// Prefer the eligible credential whose applicable quota resets first.
    EarliestReset,
    Sticky,
    /// Rotate when assigning an unbound session; reuse its eligible credential
    /// on later requests. Reassign when the pin is missing, expired or no longer
    /// usable/allowed. Without a stable session, use ordinary round-robin.
    RoundRobinAffinity,
}
/// Immutable credential execution configuration, separate from mutable secret
/// material. No stale second copy of secret/version/expiry in a Store Model.
pub struct CredentialData {
    pub id: String,
    pub provider_id: String,
    pub label: Option<String>,
    pub auth_kind: String,
    pub enabled: bool,
    pub metadata: serde_json::Value,
    /// Already resolved effective client, shared by equal connection parameters.
    pub client: Arc<dyn OutboundClient>,
    /// Same profile forced to HTTP/1.1 for RFC 6455 upgrades.
    pub websocket_client: Arc<dyn OutboundClient>,
    /// Secret, expiry and lifecycle status, versioned together. Reused across
    /// configuration reloads for the same live credential.
    pub state: Arc<CredentialState>,
    /// Declared once at assembly by the channel's `QuotaModel` from this
    /// credential's auth kind and metadata. Reported dimensions receive their
    /// values from observations; Counted ones are metered in Store windows.
    /// Operator limits follow as synthetic Counted `limit:{quota_id}`
    /// dimensions. Empty when the channel models no quota and no limit
    /// covers the credential.
    pub quota: Vec<QuotaDimension>,
    /// The operator limits behind the synthetic dimensions, for model
    /// filtering at charge time and for status/reset. Ordered by quota id.
    pub limits: Vec<Arc<crate::credential_limit::CredentialLimit>>,
}

pub struct RewriteRuleSetData {
    pub entity: Arc<upstream::rewrite_rule_set::Model>,
    pub rules: Vec<Arc<RewriteRuleData>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewritePhase {
    Request,
    Response,
    Both,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    Key(String),
    Index(usize),
    Wildcard,
}
/// Compiled target shape. Execution-data compilation must reject Query with
/// Response/Both phase and reject event filters on Header/Query targets.
pub enum RewriteTarget {
    Body {
        paths: Option<Vec<Vec<PathSegment>>>,
    },
    Header {
        name: http::HeaderName,
    },
    /// Exact decoded parameter name; only valid for the request phase.
    Query {
        name: String,
    },
}
pub struct RewriteRuleData {
    pub entity: Arc<upstream::rewrite_rule::Model>,
    pub phase: RewritePhase,
    pub pattern: Regex,
    pub action: crate::rewrite::RuleAction,
    pub target: RewriteTarget,
    pub operation_keys: Option<HashSet<OperationKey>>,
    pub model_matcher: Option<Regex>,
    pub header_matcher: Option<Regex>,
    pub event_matcher: Option<Regex>,
    pub body_condition: Option<crate::rewrite::BodyCondition>,
    pub header_condition: Option<crate::rewrite::HeaderCondition>,
}

#[cfg(test)]
mod model_placeholder_tests {
    use super::fill_model;

    #[test]
    fn the_model_fills_the_placeholder_as_one_path_segment() {
        assert_eq!(
            fill_model(
                "https://r.example/v1beta/models/{model}:generateContent",
                Some("gemini-3.1-pro")
            ),
            "https://r.example/v1beta/models/gemini-3.1-pro:generateContent"
        );
        assert_eq!(
            fill_model("https://r.example/{model}/chat", Some("org/model a")),
            "https://r.example/org%2Fmodel%20a/chat"
        );
        assert_eq!(
            fill_model("https://r.example/{model}", None),
            "https://r.example/{model}"
        );
        assert_eq!(
            fill_model("https://r.example/chat", Some("m")),
            "https://r.example/chat"
        );
    }
}
