//! Shared diagnostics traversal; vendor request rules live with their dialect.
use crate::*;
use serde::Deserialize;
mod content;
mod fields;
pub(crate) struct Walk {
    #[cfg(any(feature = "openai", feature = "chat"))]
    dialect: Dialect,
    pub(crate) vendor: Vendor,
    #[cfg(feature = "chat")]
    now: Option<u64>,
    warnings: Vec<Warning>,
}
impl Walk {
    pub(crate) fn warn(&mut self, kind: WarningKind, path: &str, message: &str) {
        self.warnings.push(Warning::new(kind, path, message));
    }
    pub(crate) fn is_chat(&self) -> bool {
        #[cfg(feature = "chat")]
        {
            self.dialect == Dialect::OpenaiChat
        }
        #[cfg(not(feature = "chat"))]
        {
            false
        }
    }
    pub(crate) fn is_responses(&self) -> bool {
        #[cfg(feature = "openai")]
        {
            self.dialect == Dialect::Openai
        }
        #[cfg(not(feature = "openai"))]
        {
            false
        }
    }
}
pub(crate) fn for_request(request: &GenerateRequest, now: Option<u64>) -> Result<Vec<Warning>> {
    #[cfg(not(feature = "chat"))]
    let _ = now;
    let mut walk = Walk {
        #[cfg(any(feature = "openai", feature = "chat"))]
        dialect: request.model.dialect,
        vendor: request.model.dialect.vendor(),
        #[cfg(feature = "chat")]
        now,
        warnings: Vec::new(),
    };
    let options = &request.provider_options;
    if walk.vendor != Vendor::Claude && options.claude != options::ClaudeOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.claude",
            "Claude options are not sent to this vendor",
        );
    }
    if walk.vendor != Vendor::Openai && options.openai != options::OpenaiOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai",
            "OpenAI options are not sent to this vendor",
        );
    }
    if walk.vendor != Vendor::Gemini && options.gemini != options::GeminiOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.gemini",
            "Gemini options are not sent to this vendor",
        );
    }
    if walk.is_chat() && options.openai.responses != options::ResponsesOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai.responses",
            "Responses options are not sent to Chat Completions",
        );
    }
    if walk.is_responses() && options.openai.chat != options::ChatOptions::default() {
        walk.warn(
            WarningKind::Compatibility,
            "provider_options.openai.chat",
            "Chat options are not sent to Responses",
        );
    }
    #[cfg(feature = "claude")]
    crate::dialects::claude::warnings::for_request(&mut walk, request);

    #[cfg(feature = "openai")]
    crate::dialects::openai::warnings::for_request(&mut walk, request);
    #[cfg(feature = "chat")]
    crate::dialects::chat::warnings::for_request(&mut walk, request);
    #[cfg(feature = "gemini")]
    crate::dialects::gemini::warnings::for_request(&mut walk, request);
    if matches!(&request.settings.tool_choice, Some(ToolChoice::Unknown(_))) {
        walk.warn(
            WarningKind::Unsupported,
            "settings.tool_choice",
            "Unknown tool choice is omitted",
        );
    }
    for (index, modality) in request.settings.output_modalities.iter().enumerate() {
        if matches!(modality, Modality::Unknown(_)) {
            walk.warn(
                WarningKind::Unsupported,
                &format!("settings.output_modalities[{index}]"),
                "Unknown output modality is omitted",
            );
        }
    }
    for (index, message) in request.messages.iter().enumerate() {
        if message.is_record_only() {
            continue;
        }
        let path = format!("messages[{index}]");
        if matches!(message.role, Role::Unknown(_)) {
            walk.warn(
                WarningKind::Unsupported,
                &format!("{path}.role"),
                "Message with an unknown persisted role is omitted",
            );
            continue;
        }
        for (index, block) in message.content.iter().enumerate() {
            walk.block(block, &message.role, &format!("{path}.content[{index}]"))?;
        }
    }
    Ok(walk.warnings)
}

#[derive(Deserialize)]
struct NativePresence {
    native: Option<serde::de::IgnoredAny>,
    custom: Option<serde::de::IgnoredAny>,
    item: Option<serde::de::IgnoredAny>,
    chat: Option<serde::de::IgnoredAny>,
}

#[derive(Default)]
struct Presence {
    native: bool,
    custom: bool,
    item: bool,
    chat: bool,
}

fn presence(metadata: &ProviderMetadata, vendor: &Vendor) -> Result<Presence> {
    let Some(rest) = metadata.get(vendor) else {
        return Ok(Presence::default());
    };
    let decoder = serde::de::value::MapDeserializer::<_, serde_json::Error>::new(
        rest.iter().map(|(key, value)| (key.as_str(), value)),
    );
    let value = NativePresence::deserialize(decoder).map_err(Error::invalid)?;
    Ok(Presence {
        native: value.native.is_some(),
        custom: value.custom.is_some(),
        item: value.item.is_some(),
        chat: value.chat.is_some(),
    })
}

impl Walk {
    fn metadata(&mut self, metadata: &ProviderMetadata, path: &str) -> Result<()> {
        for (vendor, rest) in metadata {
            if vendor != &self.vendor && !rest.is_empty() {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    &format!("Metadata for {vendor:?} is omitted when switching vendor"),
                );
            }
        }
        if self.vendor == Vendor::Openai {
            let flags = presence(metadata, &self.vendor)?;
            if self.is_chat() && (flags.native || flags.item) {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    "Native OpenAI metadata has no Chat continuation representation",
                );
            }
            if self.is_responses() && flags.chat {
                self.warn(
                    WarningKind::Compatibility,
                    &format!("{path}.provider_metadata"),
                    "Chat continuation metadata is omitted by Responses",
                );
            }
        }
        Ok(())
    }
}
