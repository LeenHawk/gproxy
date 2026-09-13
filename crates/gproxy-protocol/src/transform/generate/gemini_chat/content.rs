use crate::{
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{gemini as g, openai::chat as c},
};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Calls {
    pub index: u64,
    pub names: BTreeMap<String, Vec<String>>,
    pub ids: BTreeMap<String, String>,
}
impl Calls {
    pub(super) fn call(
        &mut self,
        name: &str,
        id: Option<String>,
        flow: &mut IdentityFlow,
        policy: &TargetIdPolicy,
    ) -> Result<String, TransformError> {
        let source = id.clone();
        let handle = flow
            .resolve_or_allocate(
                IdentityRole::ToolCall,
                SourceIdentity::new(crate::Dialect::Gemini, id, self.index),
                policy,
            )
            .map_err(|e| TransformError::shape("identity", e.to_string()))?;
        self.index += 1;
        if let Some(source) = source
            && self.ids.insert(source, handle.emitted_id.clone()).is_some()
        {
            return Err(TransformError::shape(
                "function_call.id",
                "duplicate call identity",
            ));
        }
        self.names
            .entry(name.into())
            .or_default()
            .push(handle.emitted_id.clone());
        Ok(handle.emitted_id)
    }
    fn result(&mut self, name: &str, id: Option<String>) -> Result<String, TransformError> {
        if let Some(id) = id {
            let resolved = self.ids.get(&id).cloned().ok_or_else(|| {
                TransformError::missing_metadata("function_response.id call binding")
            })?;
            if let Some(names) = self.names.get_mut(name) {
                names.retain(|value| value != &resolved);
            }
            return Ok(resolved);
        }
        let ids = self.names.get_mut(name).ok_or_else(|| {
            TransformError::missing_metadata("function_response.name call binding")
        })?;
        if ids.len() != 1 {
            return Err(TransformError::missing_metadata(
                "ambiguous same-name function response without ID",
            ));
        }
        Ok(ids.remove(0))
    }
}
pub(super) fn unsupported_part(part: &g::Part) -> Result<(), TransformError> {
    if part.executable_code.is_some()
        || part.code_execution_result.is_some()
        || part.tool_call.is_some()
        || part.tool_response.is_some()
        || part.video_metadata.is_some()
        || part.part_metadata.is_some()
        || part.media_resolution.is_some()
    {
        return Err(TransformError::unsupported(
            "parts",
            "Gemini execution/annotated media requires a host adapter",
        ));
    }
    Ok(())
}
pub(super) fn gemini_content_to_chat(
    content: g::Content,
    report: &mut Report,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    bindings: &mut Calls,
) -> Result<Vec<c::ChatMessage>, TransformError> {
    let role = content.role.as_deref().unwrap_or("user");
    if !matches!(role, "user" | "model" | "system") {
        return Err(TransformError::shape(
            "contents.role",
            "unknown Gemini role",
        ));
    }
    let mut user = Vec::new();
    let mut assistant = Vec::new();
    let mut calls = Vec::new();
    let mut result = Vec::new();
    for part in content.parts.unwrap_or_default() {
        unsupported_part(&part)?;
        if part.thought == Some(true) {
            report.omitted("parts.thought", "Chat has no typed reasoning replay block");
            continue;
        }
        if part.thought_signature.is_some() {
            report.omitted(
                "parts.thought_signature",
                "signature replay belongs to host identity state",
            );
        }
        if let Some(text) = part.text {
            if role == "model" {
                assistant.push(c::AssistantContentPart::Text(
                    c::TextPart::builder(c::TextPartType::Text, text).build(),
                ));
            } else {
                user.push(c::UserContentPart::Text(
                    c::TextPart::builder(c::TextPartType::Text, text).build(),
                ));
            }
        }
        if let Some(blob) = part.inline_data {
            if role != "user" {
                return Err(TransformError::unsupported(
                    "parts.inline_data",
                    "Chat non-user history has no multimedia parts",
                ));
            }
            user.push(super::media::to_chat(blob)?);
        }
        if let Some(file) = part.file_data {
            if role == "user"
                && file
                    .mime_type
                    .as_deref()
                    .is_some_and(|m| m.starts_with("image/"))
                && file.file_uri.starts_with("https://")
            {
                user.push(c::UserContentPart::Image(
                    c::ImagePart::builder(
                        c::ImagePartType::ImageUrl,
                        c::ImageUrl::builder(file.file_uri).build(),
                    )
                    .build(),
                ));
            } else {
                return Err(TransformError::missing_metadata(
                    "file_data requires resource bytes or supported public image URL",
                ));
            }
        }
        if let Some(call) = part.function_call {
            if role != "model" {
                return Err(TransformError::shape(
                    "function_call",
                    "function calls require model role",
                ));
            }
            let id = bindings.call(&call.name, call.id, flow, policy)?;
            calls.push(c::MessageToolCall::Function(
                c::ChatToolCall::builder(
                    id,
                    c::FunctionCall::builder(
                        serde_json::to_string(&call.args.unwrap_or_default())?,
                        call.name,
                    )
                    .build(),
                    c::ChatToolCallType::Function,
                )
                .build(),
            ));
        }
        if let Some(response) = part.function_response {
            if role != "user"
                || response.parts.is_some()
                || response.will_continue == Some(true)
                || response.scheduling.is_some()
            {
                return Err(TransformError::unsupported(
                    "function_response",
                    "non-user, multimodal or asynchronous tool output requires adapter",
                ));
            }
            flush_user(&mut user, &mut result);
            let id = bindings.result(&response.name, response.id)?;
            result.push(c::ChatMessage::Tool(
                c::ToolMessage::builder(
                    c::ToolRole::Tool,
                    c::TextContent::Text(serde_json::to_string(&response.response)?),
                    id,
                )
                .build(),
            ));
        }
    }
    if role == "model" {
        let mut message = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
        if !assistant.is_empty() {
            message.content = Some(Some(c::AssistantContent::Parts(assistant)));
        }
        if !calls.is_empty() {
            message.tool_calls = Some(calls);
        }
        result.push(c::ChatMessage::Assistant(message));
    } else if role == "system" {
        let parts = user
            .into_iter()
            .map(|part| match part {
                c::UserContentPart::Text(text) => Ok(text),
                c::UserContentPart::Image(_)
                | c::UserContentPart::File(_)
                | c::UserContentPart::InputAudio(_) => Err(TransformError::unsupported(
                    "system_instruction",
                    "system must be text only",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        result.push(c::ChatMessage::System(
            c::SystemMessage::builder(c::SystemRole::System, c::TextContent::Parts(parts)).build(),
        ));
    } else {
        flush_user(&mut user, &mut result);
    }
    Ok(result)
}
fn flush_user(parts: &mut Vec<c::UserContentPart>, out: &mut Vec<c::ChatMessage>) {
    if !parts.is_empty() {
        out.push(c::ChatMessage::User(
            c::UserMessage::builder(
                c::UserRole::User,
                c::UserContent::Parts(std::mem::take(parts)),
            )
            .build(),
        ));
    }
}
pub(super) fn chat_text(content: &c::TextContent) -> String {
    match content {
        c::TextContent::Text(text) => text.clone(),
        c::TextContent::Parts(parts) => parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(""),
    }
}
pub(super) fn assistant(
    message: &c::AssistantMessage,
    report: &mut Report,
) -> Result<Vec<g::Part>, TransformError> {
    let mut parts = Vec::new();
    if message.audio.as_ref().and_then(Option::as_ref).is_some() {
        return Err(TransformError::missing_metadata(
            "assistant audio/legacy-call requires replay binding",
        ));
    }
    if let Some(Some(content)) = &message.content {
        match content {
            c::AssistantContent::Text(text) => {
                parts.push(g::Part::builder().text(text.clone()).build())
            }
            c::AssistantContent::Parts(values) => {
                for value in values {
                    let text = match value {
                        c::AssistantContentPart::Text(text) => &text.text,
                        c::AssistantContentPart::Refusal(refusal) => {
                            report.changed("refusal", "Gemini history represents refusal as text");
                            &refusal.refusal
                        }
                    };
                    parts.push(g::Part::builder().text(text.clone()).build());
                }
            }
        }
    }
    if let Some(Some(refusal)) = &message.refusal {
        report.changed("refusal", "Gemini history represents refusal as text");
        parts.push(g::Part::builder().text(refusal.clone()).build());
    }
    if let Some(Some(call)) = &message.function_call {
        if message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
        {
            return Err(TransformError::shape(
                "function_call",
                "legacy/current calls conflict",
            ));
        }
        let args: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&call.arguments).map_err(|error| {
                TransformError::shape("function_call.arguments", error.to_string())
            })?;
        parts.push(
            g::Part::builder()
                .function_call(
                    g::FunctionCall::builder(call.name.clone())
                        .args(args)
                        .build(),
                )
                .build(),
        );
    }
    for call in message.tool_calls.iter().flatten() {
        match call {
            c::MessageToolCall::Function(call) => {
                if call.id.is_empty() {
                    return Err(TransformError::shape("call.id", "empty call ID"));
                }
                let args: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&call.function.arguments).map_err(|e| {
                        TransformError::shape(
                            "function.arguments",
                            format!("object JSON required: {e}"),
                        )
                    })?;
                parts.push(
                    g::Part::builder()
                        .function_call(
                            g::FunctionCall::builder(call.function.name.clone())
                                .id(call.id.clone())
                                .args(args)
                                .build(),
                        )
                        .build(),
                );
            }
            c::MessageToolCall::Custom(_) => {
                return Err(TransformError::unsupported(
                    "custom_tool",
                    "Gemini functions require JSON-object input and lack custom grammar tools",
                ));
            }
        }
    }
    Ok(parts)
}
