//! Native signed pieces are stored as their original declared DTOs, separately
//! from the small identity record. They are never translated into foreign opaque fields.
use super::GenerationStateAccess;
use crate::{
    Dialect,
    capability::StateStore,
    transform::{
        TransformError, TransformErrorKind,
        generate::{
            claude_responses::{ClaudeRequestContext, RestoredClaudeThinking},
            gemini_responses::{GeminiReplayContext, RestoredGeminiPart},
        },
        identity::{
            IdentityRole, IdentityStateRecord, OpaqueField, OpaqueSignature, OutputItemKind,
        },
    },
    wire::{DeclaredFields, claude::content::ThinkingBlock, gemini as g, openai::responses as r},
};
impl<S: StateStore> GenerationStateAccess<'_, S> {
    fn signature(
        &self,
        field: OpaqueField,
        value: String,
        model: Option<&str>,
    ) -> Result<OpaqueSignature, TransformError> {
        if model.is_some_and(|model| model != self.target.model) {
            return Err(TransformError::invalid_result(
                "signature.model",
                "native signed result does not match bound original model",
            ));
        }
        OpaqueSignature::new(
            field,
            value,
            self.target
                .origin
                .clone()
                .ok_or_else(|| TransformError::missing_metadata("signature.origin"))?,
            self.target.model.clone(),
        )
        .map_err(|e| TransformError::invalid_result("signature", e.to_string()))
    }
    pub(super) fn attach_claude(
        &self,
        record: &mut IdentityStateRecord,
        block: ThinkingBlock,
        model: Option<&str>,
        payloads: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), TransformError> {
        let block = block.into_declared();
        record.opaque_signature = Some(self.signature(
            OpaqueField::ClaudeThinkingSignature,
            block.signature.clone(),
            model,
        )?);
        let id = record.client_item_id.as_deref().ok_or_else(|| {
            TransformError::invalid_result("signature", "Claude reasoning lacks client item ID")
        })?;
        payloads.push((
            format!("{}:claude-thinking", self.key(record.role, id)?),
            serde_json::to_vec(&block)?,
        ));
        Ok(())
    }
    pub(super) fn attach_gemini(
        &self,
        record: &mut IdentityStateRecord,
        part: g::Part,
        model: Option<&str>,
        payloads: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), TransformError> {
        let part = part.into_declared();
        record.opaque_signature = Some(
            self.signature(
                OpaqueField::GeminiPartThoughtSignature,
                part.thought_signature
                    .clone()
                    .ok_or_else(|| TransformError::missing_metadata("signature"))?,
                model,
            )?,
        );
        let id = if record.role == IdentityRole::ToolCall {
            record.client_call_id.as_deref()
        } else {
            record.client_item_id.as_deref()
        }
        .ok_or_else(|| {
            TransformError::invalid_result("signature", "Gemini part lacks client identity")
        })?;
        payloads.push((
            format!("{}:gemini-part", self.key(record.role, id)?),
            serde_json::to_vec(&part)?,
        ));
        Ok(())
    }
    async fn native_piece<T: serde::de::DeserializeOwned + DeclaredFields>(
        &self,
        role: IdentityRole,
        id: &str,
        suffix: &str,
    ) -> Result<T, TransformError> {
        let key = format!("{}:{suffix}", self.key(role, id)?);
        let entry = self.store.get(self.scope, &key).await?.ok_or_else(|| {
            TransformError::new(
                TransformErrorKind::MissingState,
                "signature.native",
                "original signed piece unavailable",
            )
        })?;
        if entry.payload.len() as u64 > self.store.limits().read_bytes {
            return Err(TransformError::new(
                TransformErrorKind::Limit,
                "signature.native",
                "native piece exceeds state read limit",
            ));
        }
        if entry.expires_at.is_none_or(|time| time <= self.now) {
            return Err(TransformError::new(
                TransformErrorKind::MissingState,
                "signature.native",
                "native piece expired",
            ));
        }
        serde_json::from_slice::<T>(&entry.payload)
            .map(DeclaredFields::into_declared)
            .map_err(|e| TransformError::invalid_result("signature.native", e.to_string()))
    }
    pub async fn claude_replay(
        &self,
        request: &r::GenerateContentRequestBody,
    ) -> Result<ClaudeRequestContext, TransformError> {
        self.validate()?;
        if self.target.dialect != Dialect::Claude {
            return Err(TransformError::shape(
                "signature.target",
                "Claude binding required",
            ));
        }
        let mut context = ClaudeRequestContext {
            target: Some(self.target.clone()),
            ..Default::default()
        };
        if let Some(r::Input::Items(items)) = &request.input {
            if items.len() > self.max_records {
                return Err(TransformError::new(
                    TransformErrorKind::Limit,
                    "signature.history",
                    "too many history items",
                ));
            }
            for item in items {
                if let r::InputItem::Reasoning(item) = item {
                    let role = IdentityRole::OutputItem(OutputItemKind::Reasoning);
                    if let Some(record) = self.read(role, &item.id).await?
                        && record.opaque_signature.is_some()
                    {
                        let block: ThinkingBlock =
                            self.native_piece(role, &item.id, "claude-thinking").await?;
                        if record.opaque_signature.as_ref().is_none_or(|v| {
                            v.field != OpaqueField::ClaudeThinkingSignature
                                || v.value != block.signature
                        }) {
                            return Err(TransformError::invalid_result(
                                "signature.native",
                                "native Claude signature differs from saved identity",
                            ));
                        }
                        context.restored_thinking.insert(
                            item.id.clone(),
                            RestoredClaudeThinking {
                                state: record,
                                block,
                            },
                        );
                    }
                }
            }
        }
        Ok(context)
    }
    pub async fn gemini_replay(
        &self,
        request: &r::GenerateContentRequestBody,
    ) -> Result<GeminiReplayContext, TransformError> {
        self.validate()?;
        if self.target.dialect != Dialect::Gemini {
            return Err(TransformError::shape(
                "signature.target",
                "Gemini binding required",
            ));
        }
        let mut context = GeminiReplayContext {
            target: Some(self.target.clone()),
            ..Default::default()
        };
        if let Some(r::Input::Items(items)) = &request.input {
            if items.len() > self.max_records {
                return Err(TransformError::new(
                    TransformErrorKind::Limit,
                    "signature.history",
                    "too many history items",
                ));
            }
            for item in items {
                let (role, id) = match item {
                    r::InputItem::Reasoning(item) => (
                        IdentityRole::OutputItem(OutputItemKind::Reasoning),
                        &item.id,
                    ),
                    r::InputItem::FunctionCall(item) => (IdentityRole::ToolCall, &item.call_id),
                    _ => continue,
                };
                if let Some(record) = self.read(role, id).await?
                    && record.opaque_signature.is_some()
                {
                    let part: g::Part = self.native_piece(role, id, "gemini-part").await?;
                    if record.opaque_signature.as_ref().is_none_or(|v| {
                        v.field != OpaqueField::GeminiPartThoughtSignature
                            || Some(v.value.as_str()) != part.thought_signature.as_deref()
                    }) {
                        return Err(TransformError::invalid_result(
                            "signature.native",
                            "native Gemini signature differs from saved identity",
                        ));
                    }
                    context.parts.insert(
                        id.clone(),
                        RestoredGeminiPart {
                            state: record,
                            part,
                        },
                    );
                }
            }
        }
        Ok(context)
    }
}
impl<S: StateStore> GenerationStateAccess<'_, S> {
    pub(super) async fn restore_gemini_tool_parts(
        &self,
        request: &mut g::GenerateContentRequestBody,
        flow: &crate::transform::identity::IdentityFlow,
        policy: &crate::transform::identity::TargetIdPolicy,
    ) -> Result<(), TransformError> {
        let mut bindings = std::collections::BTreeMap::new();
        let mut native_parts = std::collections::BTreeMap::new();
        for part in request
            .contents
            .iter()
            .flat_map(|c| c.parts.iter().flatten())
        {
            let id = part
                .function_call
                .as_ref()
                .and_then(|c| c.id.as_ref())
                .or_else(|| part.function_response.as_ref().and_then(|r| r.id.as_ref()));
            let Some(id) = id else { continue };
            let client_id = flow
                .lookup_emitted_as(IdentityRole::ToolCall, id)
                .and_then(|h| h.source.source_id)
                .unwrap_or_else(|| id.clone());
            if bindings.contains_key(id) {
                continue;
            }
            if bindings.len() >= self.max_records {
                return Err(TransformError::new(
                    TransformErrorKind::Limit,
                    "signature.history",
                    "too many signed tool identities",
                ));
            }
            if let Some(record) = self.read(IdentityRole::ToolCall, &client_id).await?
                && record.opaque_signature.is_some()
            {
                let native: g::Part = self
                    .native_piece(IdentityRole::ToolCall, &client_id, "gemini-part")
                    .await?;
                if record.opaque_signature.as_ref().is_none_or(|v| {
                    v.field != OpaqueField::GeminiPartThoughtSignature
                        || Some(v.value.as_str()) != native.thought_signature.as_deref()
                }) {
                    return Err(TransformError::invalid_result(
                        "signature.native",
                        "native Gemini signature differs from saved identity",
                    ));
                }
                let call = native.function_call.as_ref().ok_or_else(|| {
                    TransformError::invalid_result(
                        "signature.native",
                        "saved tool identity has no native function",
                    )
                })?;
                if call
                    .id
                    .as_ref()
                    .is_some_and(|id| !policy.accepts_source(id))
                {
                    return Err(TransformError::unsupported(
                        "signature.function.id",
                        "original signed ID does not satisfy selected policy",
                    ));
                }
                bindings.insert(id.clone(), (call.id.clone(), call.name.clone()));
                native_parts.insert(id.clone(), native);
            }
        }
        for part in request
            .contents
            .iter_mut()
            .flat_map(|c| c.parts.iter_mut().flatten())
        {
            if let Some(call) = &part.function_call
                && let Some(id) = &call.id
                && let Some(native) = native_parts.get(id)
            {
                let original = native
                    .function_call
                    .as_ref()
                    .expect("validated native call");
                if call.name != original.name || call.args != original.args {
                    return Err(TransformError::shape(
                        "signature.function",
                        "modified function cannot reuse native signature",
                    ));
                }
                *part = native.clone();
            }
            if let Some(result) = &mut part.function_response
                && let Some(id) = &result.id
                && let Some((original_id, name)) = bindings.get(id)
            {
                if &result.name != name {
                    return Err(TransformError::shape(
                        "signature.function_result",
                        "result name differs from original native call",
                    ));
                }
                result.id = original_id.clone();
            }
        }
        Ok(())
    }
}
