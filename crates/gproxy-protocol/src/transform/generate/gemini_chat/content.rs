use crate::{
    transform::{
        Report, TransformError,
        generate::reasoning_details as rd,
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
    pub legacy: BTreeMap<String, String>,
    pub prior: BTreeMap<String, (String, String)>,
    pub(super) legacy_declared: std::collections::BTreeSet<String>,
    pub(super) legacy_names: BTreeMap<String, Vec<String>>,
}

impl Calls {
    fn legacy_call(&mut self, name: &str, id: Option<&str>) -> Result<bool, TransformError> {
        let Some(id) = id else {
            return Ok(false);
        };
        let Some(actual) = self.legacy.get(id) else {
            return Ok(false);
        };
        if actual != name || name.is_empty() || !self.legacy_declared.insert(id.into()) {
            return Err(TransformError::shape(
                "function_call.legacy",
                "name mismatch or duplicate actual legacy declaration",
            ));
        }
        self.legacy_names
            .entry(name.into())
            .or_default()
            .push(id.into());
        Ok(true)
    }
    fn legacy_result(&mut self, name: &str, id: Option<&str>) -> Result<bool, TransformError> {
        if let Some(id) = id {
            let Some(actual) = self.legacy.get(id) else {
                return Ok(false);
            };
            if actual != name || name.is_empty() {
                return Err(TransformError::shape(
                    "function_response.legacy",
                    "result differs from actual native legacy name",
                ));
            }
            if let Some(pending) = self.legacy_names.get_mut(name) {
                pending.retain(|known| known != id);
            }
            return Ok(true);
        }
        let Some(pending) = self.legacy_names.get_mut(name).filter(|v| !v.is_empty()) else {
            return Ok(false);
        };
        if pending.len() != 1 || self.names.get(name).is_some_and(|v| !v.is_empty()) {
            return Err(TransformError::missing_metadata(
                "ambiguous legacy/modern function result without client ID",
            ));
        }
        pending.pop();
        Ok(true)
    }
    pub(super) fn call(
        &mut self,
        name: &str,
        id: Option<String>,
        flow: &mut IdentityFlow,
        policy: &TargetIdPolicy,
    ) -> Result<String, TransformError> {
        let source = id.clone();
        let id =
            if let Some((original, actual_name)) = id.as_ref().and_then(|id| self.prior.get(id)) {
                if actual_name != name {
                    return Err(TransformError::shape(
                        "function_call.name",
                        "name differs from actual scoped native call",
                    ));
                }
                Some(original.clone())
            } else {
                id
            };
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
    fn result(
        &mut self,
        name: &str,
        id: Option<String>,
        policy: &TargetIdPolicy,
    ) -> Result<String, TransformError> {
        if let Some(id) = id {
            let resolved = if let Some(mapped) = self.ids.get(&id) {
                mapped.clone()
            } else if let Some((original, actual)) = self.prior.get(&id) {
                if actual != name {
                    return Err(TransformError::shape(
                        "function_response.name",
                        "name differs from actual scoped native call",
                    ));
                }
                if !policy.accepts_source(original) {
                    return Err(TransformError::missing_metadata(
                        "actual native ID violates policy; declared call history is required before remapping",
                    ));
                }
                original.clone()
            } else {
                return Err(TransformError::missing_metadata(
                    "function_response.id call binding",
                ));
            };
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

/// Gemini signatures a Chat client carries in `reasoning_details` (see
/// `reasoning_details::GEMINI`). A thought run can span stream chunks, so its
/// text is kept here until the part with its signature arrives.
#[derive(Default)]
pub(super) struct Signatures {
    run: String,
    index: i64,
}

impl Signatures {
    fn next_index(&mut self) -> i64 {
        let index = self.index;
        self.index += 1;
        index
    }
}

/// `signatures` is set for a response the client will send back, and unset
/// for a request history, whose signatures the Chat upstream cannot use.
pub(super) fn gemini_content_to_chat(
    content: g::Content,
    report: &mut Report,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    bindings: &mut Calls,
    mut signatures: Option<&mut Signatures>,
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
    let mut reasoning = String::new();
    let mut call_messages: Vec<c::AssistantMessage> = Vec::new();
    let mut details = Vec::new();
    let mut result = Vec::new();
    for mut part in content.parts.unwrap_or_default() {
        let run_part = crate::transform::generate::claude_gemini::thinking::is_run_part(&part);
        let mut call_signature = None;
        let signature = part.thought_signature.take();
        match signatures.as_deref_mut().filter(|_| role == "model") {
            Some(carried) if run_part => {
                carried.run.push_str(part.text.as_deref().unwrap_or(""));
                if let Some(signature) = signature {
                    let text = std::mem::take(&mut carried.run);
                    let index = carried.next_index();
                    details.push(rd::gemini_thought(text, signature, index));
                }
            }
            Some(carried) => {
                if !crate::transform::generate::claude_gemini::thinking::is_empty_text(&part) {
                    carried.run.clear();
                }
                if part.function_call.is_some() {
                    call_signature = signature;
                } else if signature.is_some() {
                    report.omitted(
                        "parts.thought_signature",
                        "a Gemini signature outside a thought or function call has no Chat field",
                    );
                }
            }
            None if signature.is_some() => report.omitted(
                "parts.thought_signature",
                "a Gemini signature has no meaning to a Chat upstream",
            ),
            None => {}
        }
        if part.thought == Some(true) {
            if role == "model" {
                if let Some(text) = part.text.take() {
                    reasoning.push_str(&text);
                }
            } else {
                report.omitted("parts.thought", "reasoning requires assistant role");
                part.text = None;
            }
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
            if role == "user" {
                if let Some(media) = crate::transform::optional(super::media::to_chat(blob))? {
                    user.push(media);
                }
            } else {
                report.omitted("parts.inline_data", "Chat has no media in this role");
            }
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
                report.omitted("file_data", "resource has no direct target representation");
            }
        }
        if let Some(call) = part.function_call {
            if role != "model" {
                return Err(TransformError::shape(
                    "function_call",
                    "function calls require model role",
                ));
            }
            let legacy = bindings.legacy_call(&call.name, call.id.as_deref())?;
            let function = c::FunctionCall::builder(
                serde_json::to_string(&call.args.unwrap_or_default())?,
                call.name.clone(),
            )
            .build();
            if legacy {
                let mut message = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                message.function_call = Some(Some(function));
                call_messages.push(message);
            } else {
                let id = bindings.call(&call.name, call.id, flow, policy)?;
                if let (Some(signature), Some(signatures)) =
                    (call_signature.take(), signatures.as_deref_mut())
                {
                    let index = signatures.next_index();
                    details.push(rd::gemini_call(id.clone(), signature, index));
                }
                if call_messages
                    .last()
                    .is_none_or(|v| v.function_call.is_some())
                {
                    call_messages
                        .push(c::AssistantMessage::builder(c::AssistantRole::Assistant).build());
                }
                call_messages
                    .last_mut()
                    .expect("allocated")
                    .tool_calls
                    .get_or_insert_with(Vec::new)
                    .push(c::MessageToolCall::Function(
                        c::ChatToolCall::builder(id, function, c::ChatToolCallType::Function)
                            .build(),
                    ));
            }
        }
        if let Some(response) = part.function_response {
            if role != "user" {
                continue;
            }
            if response.parts.is_some()
                || response.will_continue.is_some()
                || response.scheduling.is_some()
            {
                report.omitted(
                    "function_response.parts/scheduling/will_continue",
                    "Chat tool result only maps its response payload",
                );
            }
            flush_user(&mut user, &mut result);
            let content = serde_json::to_string(&response.response)?;
            if bindings.legacy_result(&response.name, response.id.as_deref())? {
                result.push(c::ChatMessage::Function(
                    c::FunctionMessage::builder(
                        c::FunctionRole::Function,
                        Some(content),
                        response.name,
                    )
                    .build(),
                ));
                report.changed(
                    "function_response",
                    "restored explicitly bound native legacy Chat function result",
                );
            } else {
                let id = bindings.result(&response.name, response.id, policy)?;
                result.push(c::ChatMessage::Tool(
                    c::ToolMessage::builder(c::ToolRole::Tool, c::TextContent::Text(content), id)
                        .build(),
                ));
            }
        }
    }
    if role == "model" {
        if call_messages.is_empty() {
            call_messages.push(c::AssistantMessage::builder(c::AssistantRole::Assistant).build());
        }
        if !reasoning.is_empty() {
            call_messages[0].reasoning_content = Some(Some(reasoning));
        }
        if !details.is_empty() {
            call_messages[0].reasoning_details = Some(Some(details));
        }
        if !assistant.is_empty() {
            call_messages[0].content = Some(Some(c::AssistantContent::Parts(assistant)));
        }
        result.extend(call_messages.into_iter().map(c::ChatMessage::Assistant));
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
            .filter_map(|value| crate::transform::optional(value).transpose())
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
    // Gemini signatures come back in the details this gateway gave the
    // client: a signed thought replays as its text with its signature, and a
    // call signature goes on the call it names. With a signed thought the
    // plain reasoning text is the same thought again, so it is left out.
    let gemini: Vec<_> = message
        .reasoning_details
        .iter()
        .flatten()
        .flatten()
        .filter(|d| d.format.as_deref() == Some(rd::GEMINI))
        .collect();
    let mut call_signatures = std::collections::BTreeMap::new();
    let mut signed_thought = false;
    for detail in gemini {
        match detail.type_ {
            c::ReasoningDetailKind::Text => {
                if let Some(signature) =
                    detail.signature.clone().flatten().filter(|v| !v.is_empty())
                {
                    signed_thought = true;
                    parts.push(
                        g::Part::builder()
                            .thought(true)
                            .text(detail.text.clone().flatten().unwrap_or_default())
                            .thought_signature(signature)
                            .build(),
                    );
                }
            }
            c::ReasoningDetailKind::Encrypted => {
                if let (Some(id), Some(data)) = (detail.id.clone().flatten(), detail.data.clone()) {
                    call_signatures.insert(id, data);
                }
            }
            c::ReasoningDetailKind::Summary => {}
        }
    }
    if !signed_thought
        && let Some(text) = c::visible_reasoning(
            &message.reasoning_content,
            &message.reasoning,
            &message.reasoning_details,
        )
    {
        parts.push(g::Part::builder().text(text).thought(true).build());
    }
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
                let mut part = g::Part::builder()
                    .function_call(
                        g::FunctionCall::builder(call.function.name.clone())
                            .id(call.id.clone())
                            .args(args)
                            .build(),
                    )
                    .build();
                part.thought_signature = call_signatures.remove(&call.id);
                parts.push(part);
            }
            c::MessageToolCall::Custom(_) => {
                continue;
            }
        }
    }
    Ok(parts)
}
