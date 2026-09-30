//! What a conversion needs to know about where it runs. No generation identity
//! is stored: a tool call ID is forwarded unchanged or aliased reversibly
//! (`transform::identity::tool_alias`), and an upstream's signature travels to
//! the client in its own protocol's field (`transform::generate::signature`),
//! so a later turn recovers both from what the client sends back. The store
//! and scope serve the Responses history a `previous_response_id` continues.

use crate::{capability::StateStore, transform::identity::IdentityTarget};
use std::{collections::BTreeMap, time::SystemTime};

pub struct GenerationStateAccess<'a, S: StateStore> {
    pub store: &'a S,
    pub scope: &'a S::Scope,
    pub target: IdentityTarget,
    pub conversation_key: String,
    pub expires_at: SystemTime,
    pub now: SystemTime,
}

/// Identity/name facts for replaying tool calls and results, all taken from
/// the client's history. Keys remain the exact client IDs.
#[derive(Debug, Default)]
pub struct GenerationToolReplay {
    pub names: BTreeMap<String, String>,
    pub kinds: BTreeMap<String, super::ToolCallKind>,
    /// Native Chat forms of aliased calls replayed to a Chat upstream, keyed
    /// by the client alias.
    pub chat_forms: BTreeMap<String, ChatCallForm>,
    /// The upstream ID each alias names.
    pub original_call_ids: BTreeMap<String, String>,
}

mod chat_form;
pub use chat_form::ChatCallForm;
