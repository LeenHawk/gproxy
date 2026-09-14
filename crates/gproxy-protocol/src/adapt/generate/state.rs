//! Durable identity association before client exposure. Host scopes must bind
//! principal and upstream; the explicit prefix additionally binds conversation.
use super::{GenerationProgress, identity_facts::IdentityFacts};
use crate::{
    Dialect,
    capability::{CasResult, StateStore, StateWrite, Version},
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
    pub fn validate(&self) -> Result<(), TransformError> {
        if self.conversation_key.is_empty()
            || self
                .target
                .origin
                .as_ref()
                .is_none_or(|v| v.trim().is_empty())
            || self.target.model.trim().is_empty()
            || self.expires_at <= self.now
            || self.max_records == 0
        {
            return Err(TransformError::shape(
                "generation.state",
                "nonempty scope binding, future expiry and positive record limit required",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_target(
        &self,
        dialect: Dialect,
        model: &str,
    ) -> Result<(), TransformError> {
        self.validate()?;
        if self.target.dialect != dialect || self.target.model != model {
            return Err(TransformError::shape(
                "generation.state.target",
                "state binding differs from selected generation target",
            ));
        }
        Ok(())
    }
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
        self.validate()?;
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
        if stored.schema != 1
            || (stored.identity.role == IdentityRole::ToolCall) != stored.tool_kind.is_some()
        {
            return Err(TransformError::invalid_result(
                "generation.state",
                "unsupported record schema or missing tool kind",
            ));
        }
        let record = &stored.identity;
        record
            .validate_for(&self.target)
            .map_err(|e| TransformError::invalid_result("generation.state", e.to_string()))?;
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
    /// Save all required records before yielding the response. A partial CAS failure
    /// retains applied versions in caller progress; recovery continues without POST.
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
        self.validate()?;
        if native.dialect() != self.target.dialect {
            return Err(TransformError::invalid_result(
                "generation.state",
                "native dialect differs from state binding",
            ));
        }
        let mut records = Vec::new();
        let mut native_payloads = Vec::new();
        if let Some(client_id) = client.response_id() {
            let role = IdentityRole::Response;
            let mut record = IdentityStateRecord::new(role, self.target.clone());
            record.client_item_id = Some(client_id.into());
            record.response_id = native.response_id().map(str::to_owned);
            records.push((record, None));
        }
        let originals = native.tools();
        let emitted = client.tools();
        if originals.len() != emitted.len() {
            return Err(TransformError::invalid_result(
                "generation.identity",
                "converted tool cardinality differs from native result",
            ));
        }
        // Direct pair converters preserve client function/custom-call order. Pair by
        // that concrete wire order, then verify name and the allocator's association.
        for (position, (original, client)) in originals.iter().zip(emitted).enumerate() {
            if original.name != client.name || original.kind != client.kind {
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
            records.push((record, Some(original.kind)));
        }
        for (role, id) in client.items() {
            let mut record = IdentityStateRecord::new(role, self.target.clone());
            record.client_item_id = Some(id.clone());
            record.response_id = native.response_id().map(str::to_owned);
            if let Some(handle) = flow.lookup_emitted_as(role, &id) {
                record.original_item_id = handle.source_id().map(str::to_owned);
                if let Some(block) = native.signed_claude(handle.source.logical_index) {
                    self.attach_claude(
                        &mut record,
                        block,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                }
                if let Some(part) = native.signed_gemini_reasoning(handle.source.logical_index) {
                    self.attach_gemini(
                        &mut record,
                        part,
                        native.native_model(),
                        &mut native_payloads,
                    )?;
                }
            }
            records.push((record, None));
        }
        self.save_records(records, native_payloads, progress).await
    }
    async fn save_records<N>(
        &self,
        records: Vec<(IdentityStateRecord, Option<super::ToolCallKind>)>,
        native_payloads: Vec<(String, Vec<u8>)>,
        progress: &mut GenerationProgress<N>,
    ) -> Result<(), TransformError> {
        if records.len() > self.max_records {
            return Err(limit());
        }
        let mut prepared = Vec::new();
        let mut seen = BTreeSet::new();
        let mut total = 0u64;
        for (record, tool_kind) in records {
            record
                .validate_for(&self.target)
                .map_err(|e| TransformError::invalid_result("generation.state", e.to_string()))?;
            let id = if record.role == IdentityRole::ToolCall {
                record.client_call_id.as_deref()
            } else {
                record.client_item_id.as_deref()
            }
            .ok_or_else(|| {
                TransformError::invalid_result("generation.state", "missing client ID")
            })?;
            let key = self.key(record.role, id)?;
            if !seen.insert(key.clone()) {
                return Err(TransformError::invalid_result(
                    "generation.state",
                    "duplicate client identity",
                ));
            }
            let bytes = serde_json::to_vec(&StoredIdentity {
                schema: 1,
                identity: record,
                tool_kind,
            })?;
            total = total.checked_add(bytes.len() as u64).ok_or_else(limit)?;
            if total > self.store.limits().write_bytes {
                return Err(limit());
            }
            prepared.push((key, bytes));
        }
        for (key, bytes) in native_payloads {
            if !seen.insert(key.clone()) {
                return Err(TransformError::invalid_result(
                    "generation.state",
                    "duplicate native signed record",
                ));
            }
            total = total.checked_add(bytes.len() as u64).ok_or_else(limit)?;
            if total > self.store.limits().write_bytes {
                return Err(limit());
            }
            prepared.push((key, bytes));
        }
        for (key, bytes) in prepared {
            if let Some((version, existing)) = progress.saved_identities.get(&key) {
                if existing != &bytes {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "identity changed during response recovery",
                    ));
                }
                let current = self.store.get(self.scope, &key).await?.ok_or_else(|| {
                    TransformError::new(
                        TransformErrorKind::MissingState,
                        "generation.state",
                        "previously applied record expired or disappeared",
                    )
                })?;
                if current.payload.len() as u64 > self.store.limits().read_bytes {
                    return Err(limit());
                }
                if &current.version != version
                    || current.payload.as_ref() != existing.as_slice()
                    || current.expires_at != Some(self.expires_at)
                {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "previously applied record changed",
                    ));
                }
                continue;
            }
            let result = self
                .store
                .compare_exchange(
                    self.scope,
                    &key,
                    None,
                    Some(StateWrite {
                        payload: bytes.clone().into(),
                        expires_at: Some(self.expires_at),
                    }),
                )
                .await?;
            match result {
                CasResult::Applied(Some(version)) => {
                    progress.saved_identities.insert(key, (version, bytes));
                }
                _ => {
                    return Err(TransformError::new(
                        TransformErrorKind::Conflict,
                        "generation.state",
                        "client identity already exists or CAS was not applied",
                    ));
                }
            }
        }
        Ok(())
    }
}
fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.state",
        "identity state budget exceeded",
    )
}
pub(super) type SavedIdentities = BTreeMap<String, (Version, Vec<u8>)>;

/// Explicit identity/name facts recovered for tool results. No content or opaque
/// payload is hidden in this record. Keys remain the exact client IDs.
#[derive(Debug, Default)]
pub struct GenerationToolReplay {
    pub names: BTreeMap<String, String>,
    pub kinds: BTreeMap<String, super::ToolCallKind>,
    pub original_call_ids: BTreeMap<String, String>,
    pub original_item_ids: BTreeMap<String, String>,
}
impl<S: StateStore> GenerationStateAccess<'_, S> {
    /// Declared full history supplies names first. State supplies names for
    /// truncated histories and exact original IDs when an alias was emitted.
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
        self.validate()?;
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

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredIdentity {
    schema: u16,
    identity: IdentityStateRecord,
    tool_kind: Option<super::ToolCallKind>,
}
