//! Stream aliases use separate records from immutable completed-response state.
//! Every pending write is retained across cancellation before any alias is yielded.
//! Provisional Chat call forms stay explicitly unknown. Required-ID replay is
//! blocked until the completed native observation supplies a known form; an
//! absent upstream ID is never guessed from the client alias.

use super::super::{GenerationStateAccess, ToolCallKind, state::StoredIdentity};
use crate::{
    capability::{CasResult, StateStore, StateWrite, Version},
    transform::{
        TransformError, TransformErrorKind,
        identity::{IdentityFlow, IdentityRole, IdentityStateRecord},
    },
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct ToolDeclaration {
    pub id: String,
    pub kind: ToolCallKind,
    /// Only a complete declared name; streamed Chat name fragments stay absent.
    pub name: Option<String>,
}

struct PendingWrite {
    key: String,
    bytes: Vec<u8>,
    expected: Option<Version>,
    in_flight: bool,
}

#[derive(Default)]
pub(crate) struct StreamLedger {
    saved: BTreeMap<String, (Version, Vec<u8>)>,
    tools: BTreeMap<String, ToolDeclaration>,
    pending: Option<PendingWrite>,
    failed: bool,
}

impl StreamLedger {
    pub fn observe_tools(
        &mut self,
        declarations: Vec<ToolDeclaration>,
    ) -> Result<(), TransformError> {
        for declaration in declarations {
            if declaration.id.is_empty() || declaration.name.as_ref().is_some_and(|v| v.is_empty())
            {
                return Err(invalid("empty declared tool identity/name"));
            }
            if let Some(old) = self.tools.get_mut(&declaration.id) {
                if old.kind != declaration.kind
                    || old
                        .name
                        .as_ref()
                        .zip(declaration.name.as_ref())
                        .is_some_and(|(a, b)| a != b)
                {
                    return Err(conflict("declared tool kind/name changed"));
                }
                if declaration.name.is_some() {
                    old.name = declaration.name;
                }
            } else {
                self.tools.insert(declaration.id.clone(), declaration);
            }
        }
        Ok(())
    }
    pub async fn save<S: StateStore>(
        &mut self,
        flow: &IdentityFlow,
        state: &GenerationStateAccess<'_, S>,
        signed: Option<&crate::transform::generate::gemini_responses::stream::SignedToolBindings>,
    ) -> Result<(), TransformError> {
        if self.failed {
            return Err(conflict("stream ledger failed"));
        }
        // An interrupted CAS is read back, never blindly retried. A matching
        // durable payload proves the alias is available before subsequent yield.
        if self.pending.is_some() {
            self.flush(state).await?;
        }
        let handles: Vec<_> = flow
            .handles()
            .take(state.max_records.saturating_add(1))
            .collect();
        if handles.len() > state.max_records || self.tools.len() > state.max_records {
            return Err(limit());
        }
        let native_response = handles
            .iter()
            .find(|h| h.role == IdentityRole::Response && h.source_role == IdentityRole::Response)
            .and_then(|h| h.source_id())
            .map(str::to_owned);
        let mut records = BTreeMap::new();
        let mut total = 0u64;
        for handle in handles {
            if handle.source.dialect != state.target.dialect {
                return Err(invalid("flow source differs from bound upstream"));
            }
            let declaration = (handle.role == IdentityRole::ToolCall)
                .then(|| self.tools.get(handle.client_id()))
                .flatten();
            if handle.role == IdentityRole::ToolCall && declaration.is_none() {
                // Allocating a call while buffering content does not expose it.
                // Its first concrete client event must declare its actual kind.
                continue;
            }
            let mut record = IdentityStateRecord::new(handle.role, state.target.clone());
            record.response_id = native_response.clone();
            record.conversation_id = Some(state.conversation_key.clone());
            if handle.role == IdentityRole::ToolCall {
                record.client_call_id = Some(handle.emitted_id.clone());
            } else {
                record.client_item_id = Some(handle.emitted_id.clone());
            }
            match handle.source_role {
                IdentityRole::ToolCall => record.original_call_id = handle.source.source_id.clone(),
                IdentityRole::OutputItem(_) => {
                    record.original_item_id = handle.source.source_id.clone()
                }
                IdentityRole::Response | IdentityRole::Message => {
                    record.response_id = handle.source.source_id.clone()
                }
                _ => return Err(invalid("nongeneration identity in stream flow")),
            }
            record.tool_name = declaration.and_then(|d| d.name.clone());

            self.prepare_record(
                state,
                record,
                declaration.map(|d| d.kind),
                &mut records,
                &mut total,
            )?;
        }
        if let Some(signed) = signed {
            if state.target.dialect != crate::Dialect::OpenAi {
                return Err(invalid("signed stream proof requires native Responses"));
            }
            for (client, original) in signed.iter() {
                let Some(tool) = self.tools.get(client) else {
                    continue;
                };
                if flow
                    .lookup_emitted_as(IdentityRole::ToolCall, client)
                    .is_some()
                {
                    return Err(conflict("signed identity overlaps ordinary flow"));
                }
                let mut record =
                    IdentityStateRecord::new(IdentityRole::ToolCall, state.target.clone());
                record.original_call_id = Some(original.into());
                record.client_call_id = Some(client.into());
                record.tool_name = tool.name.clone();
                record.response_id = native_response.clone();
                record.conversation_id = Some(state.conversation_key.clone());
                self.prepare_record(state, record, Some(tool.kind), &mut records, &mut total)?;
            }
        }
        if records.len() > state.max_records {
            return Err(limit());
        }
        for (key, (regular, bytes)) in records {
            if let Some((version, old)) = self.saved.get(&key) {
                if old == &bytes {
                    self.verify(state, &key, version, old).await?;
                    continue;
                }
            } else if state.store.get(state.scope, &regular).await?.is_some() {
                return Err(conflict(
                    "client alias already belongs to a completed invocation",
                ));
            }
            let expected = self.saved.get(&key).map(|(v, _)| v.clone());
            self.pending = Some(PendingWrite {
                key,
                bytes,
                expected,
                in_flight: false,
            });
            self.flush(state).await?;
        }
        Ok(())
    }
    fn prepare_record<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
        record: IdentityStateRecord,
        tool_kind: Option<ToolCallKind>,
        records: &mut BTreeMap<String, (String, Vec<u8>)>,
        total: &mut u64,
    ) -> Result<(), TransformError> {
        let id = if record.role == IdentityRole::ToolCall {
            record.client_call_id.as_ref()
        } else {
            record.client_item_id.as_ref()
        }
        .ok_or_else(|| invalid("missing client ID"))?;
        let regular = state.key(record.role, id)?;
        let key = format!("stream:{regular}");
        let stored = StoredIdentity {
            schema: 1,
            identity: record,
            tool_kind,
            chat_form: None,
        };
        if let Some((_, old)) = self.saved.get(&key) {
            let old: StoredIdentity = serde_json::from_slice(old)?;
            check_enrichment(&old, &stored)?;
        }
        let bytes = serde_json::to_vec(&stored)?;
        *total = total.checked_add(bytes.len() as u64).ok_or_else(limit)?;
        if *total > state.store.limits().write_bytes {
            return Err(limit());
        }
        if records.insert(key, (regular, bytes)).is_some() {
            return Err(conflict("duplicate stream identity"));
        }
        Ok(())
    }
    async fn verify<S: StateStore>(
        &self,
        state: &GenerationStateAccess<'_, S>,
        key: &str,
        version: &Version,
        bytes: &[u8],
    ) -> Result<(), TransformError> {
        let current = state
            .store
            .get(state.scope, key)
            .await?
            .ok_or_else(|| missing("stream alias expired or disappeared"))?;
        if current.payload.len() as u64 > state.store.limits().read_bytes {
            return Err(limit());
        }
        if &current.version != version
            || current.payload.as_ref() != bytes
            || current.expires_at != Some(state.expires_at)
        {
            return Err(conflict("saved stream alias changed"));
        }
        Ok(())
    }
    async fn flush<S: StateStore>(
        &mut self,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| invalid("no pending stream write"))?;
        let version = if pending.in_flight {
            let current = state
                .store
                .get(state.scope, &pending.key)
                .await?
                .ok_or_else(|| {
                    missing("unacknowledged stream CAS has no durable result; it cannot be retried")
                })?;
            if current.payload.len() as u64 > state.store.limits().read_bytes {
                return Err(limit());
            }
            if current.payload.as_ref() != pending.bytes
                || current.expires_at != Some(state.expires_at)
                || pending.expected.as_ref() == Some(&current.version)
            {
                return Err(conflict(
                    "unacknowledged stream CAS differs from durable state",
                ));
            }
            current.version
        } else {
            pending.in_flight = true;
            match state
                .store
                .compare_exchange(
                    state.scope,
                    &pending.key,
                    pending.expected.clone(),
                    Some(StateWrite {
                        payload: pending.bytes.clone().into(),
                        expires_at: Some(state.expires_at),
                    }),
                )
                .await?
            {
                CasResult::Applied(Some(version)) => version,
                _ => {
                    self.failed = true;
                    return Err(conflict("stream alias CAS was not applied"));
                }
            }
        };
        let pending = self.pending.take().expect("retained pending CAS");
        self.saved.insert(pending.key, (version, pending.bytes));
        Ok(())
    }
}

fn check_enrichment(old: &StoredIdentity, new: &StoredIdentity) -> Result<(), TransformError> {
    if old.schema != new.schema
        || old.tool_kind != new.tool_kind
        || old.identity.target != new.identity.target
        || old.identity.role != new.identity.role
    {
        return Err(conflict("stream identity binding changed"));
    }
    if old.chat_form.is_some() && old.chat_form != new.chat_form {
        return Err(conflict("native Chat call form changed after exposure"));
    }
    let (a, b) = (&old.identity, &new.identity);
    for (old, new) in [
        (&a.original_item_id, &b.original_item_id),
        (&a.original_call_id, &b.original_call_id),
        (&a.client_item_id, &b.client_item_id),
        (&a.client_call_id, &b.client_call_id),
        (&a.tool_name, &b.tool_name),
        (&a.response_id, &b.response_id),
        (&a.conversation_id, &b.conversation_id),
    ] {
        if old.is_some() && old != new {
            return Err(conflict("stream identity changed after exposure"));
        }
    }
    if a.opaque_signature != b.opaque_signature {
        return Err(conflict("stream signature changed"));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> TransformError {
    TransformError::invalid_result("generation.stream.state", message)
}

fn conflict(message: impl Into<String>) -> TransformError {
    TransformError::new(
        TransformErrorKind::Conflict,
        "generation.stream.state",
        message,
    )
}

fn missing(message: impl Into<String>) -> TransformError {
    TransformError::new(
        TransformErrorKind::MissingState,
        "generation.stream.state",
        message,
    )
}

fn limit() -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        "generation.stream.state",
        "stream identity state budget exceeded",
    )
}

#[cfg(test)]
mod tests;
