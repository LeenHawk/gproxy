//! Provider execution data and prepared rewrite matchers. The upper layer owns
//! routing, caller policy and assembly of the permitted execution target.

use crate::runtime::CredentialState;
use gproxy_channel::{BaseChannel, OutboundClient};
use gproxy_protocol::OperationKey;
use gproxy_store::entity::upstream;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub type EntityMap<M> = HashMap<String, Arc<M>>;

/// Instance-wide durable configuration revision, not a per-profile version.
/// Persistence and transactional revision allocation remain to be implemented.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConfigRevision(pub u64);

/// No Debug/Serialize: provider configuration may contain sensitive data.
/// Routing, model aliases, identity and business policies belong to the upper layer.
#[derive(Default)]
pub struct CoreData {
    pub revision: ConfigRevision,
    pub providers: EntityMap<ProviderData>,
    pub credentials: EntityMap<CredentialData>,
    pub rewrite_rule_sets: EntityMap<RewriteRuleSetData>,
}

pub struct ProviderData {
    pub entity: Arc<upstream::provider::Model>,
    pub channel: Arc<dyn BaseChannel>,
    /// References CoreData.credentials; no duplicate credential ownership.
    pub credential_ids: Vec<String>,
    pub models: Vec<Arc<upstream::provider_model::Model>>,
    pub operation_rules: Vec<Arc<upstream::operation_rule::Model>>,
    /// Ordered by sort_order then ID; shared compiled sets live in CoreData.
    pub rewrite_rule_sets: Vec<Arc<upstream::provider_rewrite_rule_set::Model>>,
    /// Runtime contract only: persistence for selection strategy is still pending.
    pub credential_strategy: CredentialStrategy,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStrategy {
    #[default]
    RoundRobin,
    Sticky,
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
    /// Reuse across configuration reloads for the same live credential.
    pub state: Arc<CredentialState>,
}

pub struct RewriteRuleSetData {
    pub entity: Arc<upstream::rewrite_rule_set::Model>,
    pub rules: Vec<RewriteRuleData>,
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
/// Compiled target shape. Upper-layer rule compilation must reject Query with
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
    pub target: RewriteTarget,
    pub operation_keys: Option<HashSet<OperationKey>>,
    pub model_matcher: Option<Regex>,
    pub header_matcher: Option<Regex>,
    pub event_matcher: Option<Regex>,
}
