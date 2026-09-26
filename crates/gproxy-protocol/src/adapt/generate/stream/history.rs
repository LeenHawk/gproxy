//! Scoped Responses continuation snapshots contain only declared native history.

use super::super::GenerationStateAccess;
use crate::{
    capability::{CasResult, StateStore, StateWrite, Version},
    transform::TransformError,
    wire::{DeclaredFields, openai::responses as r},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    schema: u16,
    response_id: String,
    input: Vec<r::input::InputItem>,
    output: Vec<r::response::ResponseOutputItem>,
}

pub(super) struct History {
    input: Vec<r::input::InputItem>,
    enabled: bool,
    pending: Option<(String, Vec<u8>)>,
    saved: Option<Version>,
    failed: bool,
    cache: Option<ResponsesHistoryCache>,
}

/// Connection-owned cache for `store=false` Responses continuation. Clone this
/// handle only within one client connection; dropping all handles loses its
/// cached content. No cache entries are written to StateStore for store=false.
#[derive(Clone)]
pub struct ResponsesHistoryCache(std::sync::Arc<std::sync::Mutex<Cache>>);

struct Cache {
    binding: Option<super::binding::StateBinding>,
    max_entries: usize,
    max_bytes: usize,
    bytes: usize,
    entries: std::collections::VecDeque<(Snapshot, std::time::SystemTime, usize)>,
}

impl ResponsesHistoryCache {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self(std::sync::Arc::new(std::sync::Mutex::new(Cache {
            binding: None,
            max_entries,
            max_bytes,
            bytes: 0,
            entries: Default::default(),
        })))
    }
    /// The first invocation to use this cache binds it to its conversation.
    /// A cache offered under another conversation, or after that binding
    /// expired, is refused rather than served.
    fn bind<S: StateStore>(
        &self,
        binding: &super::binding::StateBinding,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        let mut cache = self
            .0
            .lock()
            .map_err(|_| super::invalid("Responses history cache lock poisoned"))?;
        Self::check(cache.binding.get_or_insert_with(|| binding.clone()), state)
    }
    fn check<S: StateStore>(
        binding: &super::binding::StateBinding,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        if binding.serves(state) {
            Ok(())
        } else {
            Err(super::missing(
                "Responses history cache is bound to another conversation or expired",
            ))
        }
    }
    fn get<S: StateStore>(
        &self,
        id: &str,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<Option<Snapshot>, TransformError> {
        let cache = self
            .0
            .lock()
            .map_err(|_| super::invalid("Responses history cache lock poisoned"))?;
        // A never-bound empty cache has no content.
        let Some(binding) = &cache.binding else {
            return Ok(None);
        };
        Self::check(binding, state)?;
        Ok(cache
            .entries
            .iter()
            .find(|(value, expiry, _)| value.response_id == id && *expiry > state.now)
            .map(|(value, _, _)| value.clone()))
    }
    fn save<S: StateStore>(
        &self,
        snapshot: Snapshot,
        state: &GenerationStateAccess<'_, S>,
        size: usize,
    ) -> Result<(), TransformError> {
        let mut cache = self
            .0
            .lock()
            .map_err(|_| super::invalid("Responses history cache lock poisoned"))?;

        if cache.binding.is_none() {
            return Err(super::missing(
                "Responses cache has no conversation binding",
            ));
        }
        if size > cache.max_bytes {
            return Err(super::limit(
                "Responses snapshot exceeds connection cache budget",
            ));
        }
        if let Some((existing, expiry, _)) = cache
            .entries
            .iter()
            .find(|(v, _, _)| v.response_id == snapshot.response_id)
        {
            if *expiry != state.expires_at
                || serde_json::to_vec(existing)? != serde_json::to_vec(&snapshot)?
            {
                return Err(super::conflict("Responses cached response ID changed"));
            }
            return Ok(());
        }
        while cache.entries.len() >= cache.max_entries
            || cache.bytes.saturating_add(size) > cache.max_bytes
        {
            let (_, _, removed) = cache
                .entries
                .pop_front()
                .ok_or_else(|| super::invalid("Responses cache accounting inconsistent"))?;
            cache.bytes -= removed;
        }
        cache.bytes += size;
        cache.entries.push_back((snapshot, state.expires_at, size));
        Ok(())
    }
}

fn key<S: StateStore>(state: &GenerationStateAccess<'_, S>, id: &str) -> String {
    format!(
        "responses-history:{}:{}:{}:{}",
        state.conversation_key.len(),
        state.conversation_key,
        id.len(),
        id
    )
}

fn items(input: Option<r::input::Input>) -> Vec<r::input::InputItem> {
    use r::input::*;
    match input {
        None => vec![],
        Some(Input::Items(v)) => v,
        Some(Input::Text(text)) => vec![InputItem::Easy(
            EasyInputMessage::builder(MessageContent::Text(text), MessageRole::User).build(),
        )],
    }
}

fn output_item(
    item: r::response::ResponseOutputItem,
) -> Result<r::input::InputItem, TransformError> {
    use r::{input::InputItem as I, response::ResponseOutputItem as O};
    // The native input/output definitions differ. Select the concrete history
    // variant explicitly; never deserialize through the ambiguous untagged union.
    fn same<A: Serialize, T: serde::de::DeserializeOwned + DeclaredFields>(
        v: A,
    ) -> Result<T, TransformError> {
        Ok(serde_json::from_value::<T>(serde_json::to_value(v)?)?.into_declared())
    }
    Ok(match item.into_declared() {
        O::Message(v) => I::OutputMessage(same(v)?),
        O::FunctionCall(v) => I::FunctionCall(same(v)?),
        O::CustomToolCall(v) => I::CustomToolCall(same(v)?),
        O::ShellCall(v) => I::ShellCall(same(v)?),
        O::ApplyPatchCall(v) => I::ApplyPatchCall(same(v)?),
        O::ToolSearchCall(v) => I::ToolSearchCall(same(v)?),
        O::ToolSearchOutput(v) => I::ToolSearchOutput(same(v)?),
        O::AdditionalTools(v) => I::AdditionalTools(same(v)?),
        O::Reasoning(v) => I::Reasoning(same(v)?),
        O::ImageGenerationCall(v) => I::ImageGenerationCall(same(v)?),
        _ => {
            return Err(TransformError::unsupported(
                "previous_response_id.output",
                "this declared output has no implemented cross-protocol history mapping",
            ));
        }
    })
}

impl History {
    pub fn bind<S: StateStore>(
        &self,
        binding: &super::binding::StateBinding,
        state: &GenerationStateAccess<'_, S>,
    ) -> Result<(), TransformError> {
        if let Some(cache) = &self.cache {
            cache.bind(binding, state)?;
        }
        Ok(())
    }
    pub async fn prepare_with_cache<S: StateStore>(
        request: &r::GenerateContentRequestBody,
        state: &GenerationStateAccess<'_, S>,
        limits: crate::codec::CodecLimits,
        cache: Option<&ResponsesHistoryCache>,
    ) -> Result<(Self, r::GenerateContentRequestBody), TransformError> {
        if let Some(cache) = cache {
            cache.get("", state)?;
        }
        let mut expanded = request.clone().into_declared();
        let mut input = vec![];
        if let Some(id) = request
            .previous_response_id
            .as_ref()
            .and_then(Option::as_ref)
        {
            if id.trim().is_empty() {
                return Err(super::missing("empty previous_response_id"));
            }
            if request
                .conversation
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
            {
                return Err(TransformError::shape(
                    "previous_response_id",
                    "conversation and previous_response_id cannot be combined",
                ));
            }
            let cached = if let Some(cache) = cache {
                cache.get(id, state)?
            } else {
                None
            };
            let snapshot = if let Some(snapshot) = cached {
                snapshot
            } else {
                let entry = state.store.get(state.scope, &key(state, id)).await?
                .ok_or_else(|| super::missing("previous_response_id has no scoped declared history; supply full input with previous_response_id null"))?;
                if entry.payload.len() as u64
                    > state.store.limits().read_bytes.min(limits.max_body_bytes)
                {
                    return Err(super::limit("Responses history read budget exceeded"));
                }
                if entry.expires_at.is_none_or(|v| v <= state.now) {
                    return Err(super::missing("Responses history expired"));
                }
                let snapshot: Snapshot = serde_json::from_slice(&entry.payload)?;
                snapshot
            };
            input = snapshot
                .input
                .into_iter()
                .map(DeclaredFields::into_declared)
                .collect();
            for item in snapshot.output {
                input.push(output_item(item)?);
            }
        }
        input.extend(items(expanded.input.take()));
        if input.len() > state.max_records {
            return Err(super::limit("Responses history item budget exceeded"));
        }
        expanded.input = Some(r::input::Input::Items(input.clone()));
        expanded.previous_response_id = None;
        crate::codec::encode_json(&expanded, limits).map_err(super::codec_error)?;
        Ok((
            Self {
                input,
                enabled: request.store.flatten() != Some(false),
                pending: None,
                saved: None,
                failed: false,
                cache: cache.cloned(),
            },
            expanded,
        ))
    }
    pub async fn save<S: StateStore>(
        &mut self,
        response: &r::GenerateContentResponseBody,
        state: &GenerationStateAccess<'_, S>,
        limits: crate::codec::CodecLimits,
    ) -> Result<(), TransformError> {
        if self.failed {
            return Err(super::conflict(
                "Responses history write previously conflicted",
            ));
        }
        if !self.enabled && self.cache.is_none() {
            return Ok(());
        }
        if response.id.is_empty() {
            return Err(super::invalid("cannot save history without response ID"));
        }
        if self.input.len().saturating_add(response.output.len()) > state.max_records {
            return Err(super::limit(
                "Responses completed history item budget exceeded",
            ));
        }
        let snapshot = Snapshot {
            schema: 1,
            response_id: response.id.clone(),
            input: self.input.clone(),
            output: response.clone().into_declared().output,
        };
        let payload = serde_json::to_vec(&snapshot)?;
        if payload.len() as u64 > limits.max_body_bytes {
            return Err(super::limit("Responses history codec budget exceeded"));
        }
        if !self.enabled {
            if let Some(cache) = &self.cache {
                cache.save(snapshot, state, payload.len())?;
            }
            return Ok(());
        }
        if payload.len() as u64 > state.store.limits().write_bytes {
            return Err(super::limit("Responses history write budget exceeded"));
        }
        let key = key(state, &response.id);
        if let Some((known_key, bytes)) = &self.pending {
            if *known_key != key || *bytes != payload {
                return Err(super::conflict("pending Responses history changed"));
            }
            let entry = state.store.get(state.scope, &key).await?.ok_or_else(|| {
                super::missing("unacknowledged Responses history write has no durable result")
            })?;
            if entry.payload.len() as u64 > state.store.limits().read_bytes {
                return Err(super::limit("Responses history recovery read exceeded"));
            }
            if entry.payload.as_ref() != payload
                || entry.expires_at != Some(state.expires_at)
                || self.saved.as_ref().is_some_and(|v| *v != entry.version)
            {
                return Err(super::conflict("Responses history write changed"));
            }
            self.saved = Some(entry.version);
            if let Some(cache) = &self.cache {
                cache.save(snapshot, state, payload.len())?;
            }
            return Ok(());
        }
        self.pending = Some((key.clone(), payload.clone()));
        let size = payload.len();
        match state
            .store
            .compare_exchange(
                state.scope,
                &key,
                None,
                Some(StateWrite {
                    payload: payload.into(),
                    expires_at: Some(state.expires_at),
                }),
            )
            .await?
        {
            CasResult::Applied(Some(version)) => {
                self.saved = Some(version);
                if let Some(cache) = &self.cache {
                    cache.save(snapshot, state, size)?;
                }
                Ok(())
            }
            _ => {
                self.failed = true;
                Err(super::conflict(
                    "Responses history response ID already exists",
                ))
            }
        }
    }
}
