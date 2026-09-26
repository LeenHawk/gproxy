//! Identity facts a later turn cannot recover from what the client sends back:
//! an emitted alias, a call the upstream sent without an ID, or a signed piece.
//! An ID forwarded unchanged is never recorded. Host scopes must bind principal
//! and upstream; the explicit prefix additionally binds conversation.

use super::{GenerationProgress, identity_facts::IdentityFacts};
use crate::{
    Dialect,
    capability::{CasResult, StateStore, StateWrite},
    transform::{
        TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, IdentityStateRecord, IdentityTarget},
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::SystemTime,
};

pub struct GenerationStateAccess<'a, S: StateStore> {
    pub store: &'a S,
    pub scope: &'a S::Scope,
    pub target: IdentityTarget,
    pub conversation_key: String,
    pub expires_at: SystemTime,
    pub now: SystemTime,
    pub max_records: usize,
}

impl<S: StateStore> GenerationStateAccess<'_, S> {
    pub(super) fn key(&self, role: IdentityRole, id: &str) -> Result<String, TransformError> {
        if id.is_empty() {
            return Err(TransformError::invalid_result(
                "identity.id",
                "empty client identity",
            ));
        }
        let role = serde_json::to_string(&role)?;
        Ok(format!(
            "generate:{}:{}:{}:{}:{}:{}",
            self.conversation_key.len(),
            self.conversation_key,
            role.len(),
            role,
            id.len(),
            id
        ))
    }
    pub async fn read(
        &self,
        role: IdentityRole,
        client_id: &str,
    ) -> Result<Option<IdentityStateRecord>, TransformError> {
        Ok(self
            .read_stored(role, client_id)
            .await?
            .map(|value| value.identity))
    }
    async fn read_stored(
        &self,
        role: IdentityRole,
        client_id: &str,
    ) -> Result<Option<StoredIdentity>, TransformError> {
        let key = self.key(role, client_id)?;
        let Some(entry) = self.store.get(self.scope, &key).await? else {
            return Ok(None);
        };
        if entry.payload.len() as u64 > self.store.limits().read_bytes {
            return Err(limit());
        }
        if entry.expires_at.is_none_or(|time| time <= self.now) {
            return Err(TransformError::new(
                TransformErrorKind::MissingState,
                "generation.state",
                "missing or expired state expiry",
            ));
        }
        let stored: StoredIdentity = serde_json::from_slice(&entry.payload)
            .map_err(|e| TransformError::invalid_result("generation.state", e.to_string()))?;

        let record = &stored.identity;

        if record.role != role
            || if role == IdentityRole::ToolCall {
                record.client_call_id.as_deref() != Some(client_id)
            } else {
                record.client_item_id.as_deref() != Some(client_id)
            }
        {
            return Err(TransformError::invalid_result(
                "generation.state",
                "record does not match lookup identity",
            ));
        }
        Ok(Some(stored))
    }
    /// Save the records a later turn needs before yielding the response. A
    /// failed write keeps the ones already applied in caller progress, and a
    /// retry writes the rest without another POST.
    pub(super) async fn save_pair<N: IdentityFacts, C: IdentityFacts>(
        &self,
        native: &N,
        client: &C,
        flow: &IdentityFlow,
        progress: &mut GenerationProgress<N>,
    ) -> Result<(), TransformError> {
        self.save_pair_with_bound_ids(
            native,
            client,
            flow,
            &super::request_ids::SignedToolBindings::default(),
            progress,
        )
        .await
    }
    /// Only a direct pure converter's validated signed-native restoration may
    /// provide fixed client IDs that differ from the actual source call IDs.
    pub(super) async fn save_pair_with_bound_ids<N: IdentityFacts, C: IdentityFacts>(
        &self,
        native: &N,
        client: &C,
        flow: &IdentityFlow,
        signed_ids: &super::request_ids::SignedToolBindings,
        progress: &mut GenerationProgress<N>,
    ) -> Result<(), TransformError> {
        if native.dialect() != self.target.dialect {
            return Err(TransformError::invalid_result(
                "generation.state",
                "native dialect differs from state binding",
            ));
        }
        let mut records = Vec::new();
        let mut native_payloads = Vec::new();
        let mut chat_forms = BTreeMap::new();
        // No Response or Message record is written: nothing reads one back,
        // and a record keyed by an upstream response ID only made an upstream
        // that repeats its IDs fail every later request in the conversation.
        let originals = native.tools();
        let emitted = client.tools();
        let omitted =
            if client.dialect() == crate::Dialect::OpenAi && originals.len() != emitted.len() {
                native.omitted_custom_tools()
            } else {
                Vec::new()
            };
        let originals: Vec<_> = originals
            .iter()
            .enumerate()
            .filter(|(index, _)| !omitted.contains(index))
            .collect();
        if originals.len() != emitted.len() {
            return Err(TransformError::invalid_result(
                "generation.identity",
                "converted tool cardinality differs from native result",
            ));
        }
        // Direct pair converters preserve client function/custom-call order. Pair by
        // that concrete wire order, then verify name and the allocator's association.
        for ((position, original), client) in originals.into_iter().zip(emitted) {
            // Function-only backends execute declared custom tools through a
            // bound alias. Store the native name/kind for exact history replay.
            let custom_binding = original.kind == super::ToolCallKind::Function
                && client.kind == super::ToolCallKind::Custom
                && original.name
                    == crate::transform::generate::client_tools::custom_alias(&client.name);
            if !custom_binding && (original.name != client.name || original.kind != client.kind) {
                return Err(TransformError::invalid_result(
                    "generation.identity",
                    "tool name/order changed during conversion",
                ));
            }
            let Some(client_id) = client.call_id else {
                continue;
            };
            if let Some(handle) = flow.lookup_emitted_as(IdentityRole::ToolCall, &client_id) {
                if handle.source.dialect != native.dialect()
                    || handle.source_id() != original.call_id.as_deref()
                {
                    return Err(TransformError::invalid_result(
                        "generation.identity",
                        "allocator contradicts native call identity",
                    ));
                }
            } else if original.call_id.as_deref() != Some(client_id.as_str())
                && signed_ids.original_call_id(&client_id) != Some(&original.call_id)
            {
                return Err(TransformError::invalid_result(
                    "generation.identity",
                    "changed tool ID has no native association",
                ));
            }
            let mut record = IdentityStateRecord::new(IdentityRole::ToolCall, self.target.clone());
            record.original_call_id = original.call_id.clone();
            record.original_item_id = original.item_id.clone();
            if let Some(form) = original.chat_form {
                chat_forms.insert(client_id.clone(), form);
            }
            record.client_call_id = Some(client_id);
            record.client_item_id = client.item_id;
            record.tool_name = Some(original.name.clone());
            record.response_id = native.response_id().map(str::to_owned);
            if let Some(part) = native.signed_gemini_tool(position) {
                self.attach_gemini(
                    &mut record,
                    part,
                    native.native_model(),
                    &mut native_payloads,
                )?;
            }
            // A call forwarded under the upstream's own ID has nothing to
            // remember: the client sends that ID back and it goes upstream
            // unchanged. Only an alias, a call the upstream sent without an
            // ID, or a signed call still needs its native facts next turn.
            if record.original_call_id == record.client_call_id && record.opaque_signature.is_none()
            {
                continue;
            }
            records.push((record, Some(original.kind)));
        }
        for (role, id) in client.items() {
            if matches!(role, IdentityRole::Response | IdentityRole::Message) {
                continue;
            }
            let mut record = IdentityStateRecord::new(role, self.target.clone());
            record.client_item_id = Some(id.clone());
            record.response_id = native.response_id().map(str::to_owned);
            if let Some(handle) = flow.lookup_emitted_as(role, &id) {
                if handle.source.dialect != native.dialect() {
                    return Err(TransformError::invalid_result(
                        "generation.identity",
                        "output item flow differs from native dialect",
                    ));
                }
                match handle.source_role {
                    IdentityRole::ToolCall => {
                        record.original_call_id = handle.source_id().map(str::to_owned)
                    }
                    IdentityRole::OutputItem(_) => {
                        record.original_item_id = handle.source_id().map(str::to_owned)
                    }
                    IdentityRole::Response | IdentityRole::Message => {
                        if let Some(id) = handle.source_id() {
                            if record
                                .response_id
                                .as_deref()
                                .is_some_and(|known| known != id)
                            {
                                return Err(TransformError::invalid_result(
                                    "generation.identity",
                                    "source response identity contradicts actual native response",
                                ));
                            }
                            record.response_id = Some(id.into());
                        }
                    }
                    IdentityRole::Resource => {
                        return Err(TransformError::invalid_result(
                            "generation.identity",
                            "resource identity cannot become generation output",
                        ));
                    }
                }
                if role
                    == IdentityRole::OutputItem(
                        crate::transform::identity::OutputItemKind::Reasoning,
                    )
                    && let Some(block) = native.signed_claude(handle.source.logical_index)
                {
                    self.attach_claude(
                        &mut record,
                        block,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                }
                if role
                    == IdentityRole::OutputItem(
                        crate::transform::identity::OutputItemKind::Reasoning,
                    )
                    && client.dialect() == Dialect::Claude
                    && let Some(part) = native.signed_gemini_thinking(handle.source.logical_index)
                {
                    self.attach_gemini(
                        &mut record,
                        part,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                } else if role
                    == IdentityRole::OutputItem(
                        crate::transform::identity::OutputItemKind::Reasoning,
                    )
                    && let Some(part) = native.signed_gemini_reasoning(handle.source.logical_index)
                {
                    self.attach_gemini(
                        &mut record,
                        part,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                }
                if role
                    == IdentityRole::OutputItem(
                        crate::transform::identity::OutputItemKind::ImageGenerationCall,
                    )
                    && let Some(part) = native.signed_gemini_image(handle.source.logical_index)
                {
                    self.attach_gemini(
                        &mut record,
                        part,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                }
            }
            // An output item is only ever read back for its signed native
            // piece; an unsigned one is replayed from what the client sends.
            if record.opaque_signature.is_none() {
                continue;
            }
            records.push((record, None));
        }
        self.save_records_with_chat_forms(records, native_payloads, &chat_forms, progress)
            .await
    }
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.state",
        "identity state budget exceeded",
    )
}

/// Records one invocation already wrote, so a resumed save skips them.
pub(super) type SavedIdentities = BTreeMap<String, Vec<u8>>;

/// Explicit identity/name facts recovered for tool results. No content or opaque
/// payload is hidden in this record. Keys remain the exact client IDs.
#[derive(Debug, Default)]
pub struct GenerationToolReplay {
    pub names: BTreeMap<String, String>,
    pub kinds: BTreeMap<String, super::ToolCallKind>,
    /// Known native Chat forms, keyed by the original client alias. Unknown
    /// provisional forms cannot be used to replay a result.
    pub chat_forms: BTreeMap<String, ChatCallForm>,
    pub original_call_ids: BTreeMap<String, String>,
    pub original_item_ids: BTreeMap<String, String>,
}

impl<S: StateStore> GenerationStateAccess<'_, S> {
    /// Declared full history supplies names first. State supplies the native
    /// facts of an emitted alias or of a call the upstream sent without an ID.
    /// A client ID with no record is left out of the replay, so it is
    /// forwarded unchanged: an upstream ID the client saw as-is is never
    /// recorded, and nothing needs translating back.
    pub async fn recover_tools(
        &self,
        client_ids: &[String],
        history_names: &BTreeMap<String, String>,
    ) -> Result<GenerationToolReplay, TransformError> {
        self.recover_tools_inner(client_ids, history_names, true)
            .await
    }
    pub(super) async fn recover_tools_inner(
        &self,
        client_ids: &[String],
        history_names: &BTreeMap<String, String>,
        require_names: bool,
    ) -> Result<GenerationToolReplay, TransformError> {
        if client_ids.len() > self.max_records || history_names.len() > self.max_records {
            return Err(limit());
        }
        let mut output = GenerationToolReplay {
            names: history_names.clone(),
            ..Default::default()
        };
        for id in client_ids {
            let saved = self.read_stored(IdentityRole::ToolCall, id).await?;
            if let Some(saved) = saved {
                let legacy = if self.target.dialect == Dialect::OpenAiChat {
                    let form = saved
                        .chat_form
                        .ok_or_else(|| missing_form("native Chat call form is not yet known"))?;
                    output.chat_forms.insert(id.clone(), form);
                    form == ChatCallForm::LegacyFunction
                } else {
                    false
                };
                if self.target.dialect != Dialect::Gemini
                    && !legacy
                    && saved.identity.original_call_id.is_none()
                {
                    return Err(missing_form(
                        "original native tool-call ID is unavailable; client aliases cannot replace it",
                    ));
                }
                if legacy
                    && saved
                        .identity
                        .tool_name
                        .as_ref()
                        .is_none_or(|name| name.is_empty())
                {
                    return Err(missing_form(
                        "legacy Chat function requires its actual complete name",
                    ));
                }
                if let Some(kind) = saved.tool_kind {
                    output.kinds.insert(id.clone(), kind);
                }
                let saved = saved.identity;
                if let Some(name) = saved.tool_name {
                    if output.names.get(id).is_some_and(|known| known != &name) {
                        return Err(TransformError::shape(
                            "history.tool_name",
                            "declared history conflicts with stored identity",
                        ));
                    }
                    output.names.insert(id.clone(), name);
                }
                if saved.opaque_signature.is_none()
                    && let Some(original) = saved.original_call_id
                {
                    output.original_call_ids.insert(id.clone(), original);
                }
                if let Some(original) = saved.original_item_id {
                    output.original_item_ids.insert(id.clone(), original);
                }
            }
            if require_names && output.names.get(id).is_none_or(|name| name.is_empty()) {
                return Err(TransformError::new(
                    TransformErrorKind::MissingState,
                    "history.tool_name",
                    "tool result has neither declared history nor a scoped saved name",
                ));
            }
        }
        Ok(output)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct StoredIdentity {
    pub(super) schema: u16,
    pub(super) identity: IdentityStateRecord,
    pub(super) tool_kind: Option<super::ToolCallKind>,
    // Absent is unknown for a Chat tool record, not an implicit legacy flag.
    // It is inapplicable to other dialects and non-tool identity roles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) chat_form: Option<ChatCallForm>,
}

mod chat_form;
mod save;
pub use chat_form::ChatCallForm;
use chat_form::missing_form;
