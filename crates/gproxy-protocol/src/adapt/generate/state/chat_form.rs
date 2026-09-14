use super::*;
/// Actual native Chat wire form. Missing source IDs never imply legacy calls.
/// A Modern observation can still lack its native ID; that fact is retained,
/// but cannot authorize a guessed tool-result ID. LegacyFunction uses its
/// actual name and has no native call/item ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatCallForm {
    Modern,
    LegacyFunction,
}
impl StoredIdentity {
    pub(super) fn validate_shape(&self) -> Result<(), TransformError> {
        if self.schema != 1
            || (self.identity.role == IdentityRole::ToolCall) != self.tool_kind.is_some()
        {
            return Err(TransformError::invalid_result(
                "generation.state",
                "unsupported record schema or missing tool kind",
            ));
        }
        if let Some(form) = self.chat_form {
            if self.identity.target.dialect != Dialect::OpenAiChat
                || self.identity.role != IdentityRole::ToolCall
                || self.identity.opaque_signature.is_some()
            {
                return Err(TransformError::invalid_result(
                    "generation.state.chat_form",
                    "Chat call form requires an unsigned native Chat tool record",
                ));
            }
            if form == ChatCallForm::LegacyFunction
                && (self.tool_kind != Some(super::super::ToolCallKind::Function)
                    || self.identity.original_call_id.is_some()
                    || self.identity.original_item_id.is_some())
            {
                return Err(TransformError::invalid_result(
                    "generation.state.chat_form",
                    "legacy Chat functions have no native call/item ID and are not custom tools",
                ));
            }
        }
        Ok(())
    }
}
pub(super) fn missing_form(message: &'static str) -> TransformError {
    TransformError::new(
        TransformErrorKind::MissingState,
        "history.native_call",
        message,
    )
}
impl GenerationToolReplay {
    pub(crate) fn original_chat_calls(&self) -> BTreeMap<String, (String, String)> {
        self.chat_forms
            .iter()
            .filter(|(_, form)| **form == ChatCallForm::Modern)
            .filter_map(|(client, _)| {
                self.original_call_ids
                    .get(client)
                    .zip(self.names.get(client))
                    .map(|(id, name)| (client.clone(), (id.clone(), name.clone())))
            })
            .collect()
    }
    pub(crate) fn legacy_chat_calls(&self) -> BTreeMap<String, String> {
        self.chat_forms
            .iter()
            .filter(|(_, form)| **form == ChatCallForm::LegacyFunction)
            .filter_map(|(id, _)| self.names.get(id).map(|name| (id.clone(), name.clone())))
            .collect()
    }
}
