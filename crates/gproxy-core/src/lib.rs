//! Draft core data structures.

use std::{collections::HashMap, sync::Arc};

use gproxy_channel::{BaseChannel, OutboundClient};
use gproxy_store::entity::upstream::{
    credential, provider, provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set,
};
use serde_json::Value;

/// The execution core owned by the application.
///
/// Request order: protocol conversion -> regex rewriting at selected JSON paths
/// -> channel-specific upstream shaping and invocation.
/// With no applicable rewrite rules, the rewrite stage passes through the
/// original body or stream without decoding, buffering, or re-encoding it.
pub struct Core {
    pub data: CoreData,
}

/// Loaded upstream runtime data, indexed by provider ID.
///
/// Rewrite rule sets are shared by their provider attachments.
pub struct CoreData {
    pub providers: HashMap<String, ProviderData>,
    /// Indexed by rule-set ID.
    pub rewrite_rule_sets: HashMap<String, RewriteRuleSetData>,
}

/// A provider configuration, its channel, and its credential pool.
pub struct ProviderData {
    pub entity: provider::Model,
    pub channel: Arc<dyn BaseChannel>,
    /// Indexed by credential ID.
    pub credentials: HashMap<String, CredentialData>,
    /// Ordered by (sort_order, id); references CoreData.rewrite_rule_sets.
    pub rewrite_rule_sets: Vec<provider_rewrite_rule_set::Model>,
}

pub struct RewriteRuleSetData {
    pub entity: rewrite_rule_set::Model,
    /// Ordered by (sort_order, id).
    pub rules: Vec<rewrite_rule::Model>,
}

/// A persisted credential entity and its assembled runtime data.
pub struct CredentialData {
    pub entity: credential::Model,
    /// Decrypted channel input; entity.secret contains the sealed representation.
    pub secret: Value,
    /// The client selected by connection configuration; credentials may share it.
    pub client: Arc<dyn OutboundClient>,
}
