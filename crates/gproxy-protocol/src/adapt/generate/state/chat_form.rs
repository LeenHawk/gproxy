use super::*;

/// Actual native Chat wire form. LegacyFunction uses its actual name and has
/// no native call/item ID; its client alias says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatCallForm {
    Modern,
    LegacyFunction,
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
