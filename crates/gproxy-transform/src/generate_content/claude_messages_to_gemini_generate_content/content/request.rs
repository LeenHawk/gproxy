use std::collections::{BTreeMap, BTreeSet};

use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{functions, media, native};
use crate::generate_content::claude_messages_to_gemini_generate_content::tools;

pub(crate) fn system(
    system: Option<claude::SystemPrompt>,
) -> Result<Option<gemini::Content>, TransformError> {
    let Some(system) = system else {
        return Ok(None);
    };
    let parts = match system {
        claude::StringOrArray::String(text) => vec![super::text_part(text, Default::default())],
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .map(system_part)
            .collect::<Result<Vec<_>, TransformError>>()?,
        claude::StringOrArray::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude system",
                raw.to_string(),
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Claude system",
                "future content",
            ));
        }
    };
    Ok(Some(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
        rest: Default::default(),
    }))
}

pub(crate) fn request_messages(
    messages: Vec<claude::MessageParam>,
) -> Result<Vec<gemini::Content>, TransformError> {
    let mut names = BTreeMap::new();
    let mut native_ids = BTreeSet::new();
    let mut output = Vec::new();
    for message in messages {
        let role = role(message.role)?;
        let blocks = match message.content {
            claude::StringOrArray::String(text) => {
                vec![claude::ContentBlockParam::Text(claude::TextBlock {
                    text,
                    type_: claude::TextBlockType::Text,
                    cache_control: None,
                    citations: None,
                    rest: Default::default(),
                })]
            }
            claude::StringOrArray::Array(blocks) => blocks,
            claude::StringOrArray::Raw(raw) => {
                return Err(TransformError::unsupported(
                    "Claude content",
                    raw.to_string(),
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Claude content",
                    "future shape",
                ));
            }
        };
        let mut parts = Vec::new();
        let mut pending_signature = None;
        for block in blocks {
            if let Some(part) =
                block_to_part(block, &mut names, &mut native_ids, &mut pending_signature)?
            {
                parts.push(part);
            }
        }
        if !parts.is_empty() {
            output.push(content(parts, role, message.rest));
        }
    }
    Ok(output)
}

fn block_to_part(
    block: claude::ContentBlockParam,
    names: &mut BTreeMap<String, String>,
    native_ids: &mut BTreeSet<String>,
    pending_signature: &mut Option<String>,
) -> Result<Option<gemini::Part>, TransformError> {
    if let claude::ContentBlockParam::RedactedThinking(block) = &block {
        *pending_signature = Some(block.data.clone());
        return Ok(None);
    }
    Ok(Some(match block {
        claude::ContentBlockParam::Text(block) => super::text_part(block.text, block.rest),
        claude::ContentBlockParam::Thinking(block) => functions::thought(block),
        claude::ContentBlockParam::Image(block) => {
            let mut part = media::image(block.source)?;
            part.rest.extend(block.rest);
            part
        }
        claude::ContentBlockParam::Document(block) => {
            let mut part = media::document(block.source)?;
            part.rest.extend(block.rest);
            part
        }
        claude::ContentBlockParam::ToolUse(mut block) if tools::is_native_name(&block.name) => {
            native_ids.insert(block.id.clone());
            let signature =
                functions::take_signature(&mut block.caller)?.or_else(|| pending_signature.take());
            let mut part = native::call(block.id, block.input, block.rest)?;
            part.thought_signature = signature;
            part
        }
        claude::ContentBlockParam::ToolUse(mut block) => {
            names.insert(block.id.clone(), block.name.clone());
            let signature =
                functions::take_signature(&mut block.caller)?.or_else(|| pending_signature.take());
            functions::function_call(block, signature)
        }
        claude::ContentBlockParam::ServerToolUse(block)
            if tools::is_server_native_name(&block.name) =>
        {
            native_ids.insert(block.id.clone());
            native::call(block.id, block.input, block.rest)?
        }
        claude::ContentBlockParam::ToolResult(block) if native_ids.contains(&block.tool_use_id) => {
            native::result(block)?
        }
        claude::ContentBlockParam::ToolResult(block) => functions::function_result(block, names)?,
        claude::ContentBlockParam::BashCodeExecutionToolResult(block) => {
            native::request_bash_result(block)?
        }
        claude::ContentBlockParam::Raw(_)
        | claude::ContentBlockParam::ServerToolUse(_)
        | claude::ContentBlockParam::McpToolUse(_)
        | claude::ContentBlockParam::McpToolResult(_)
        | claude::ContentBlockParam::WebSearchToolResult(_)
        | claude::ContentBlockParam::WebFetchToolResult(_)
        | claude::ContentBlockParam::CodeExecutionToolResult(_)
        | claude::ContentBlockParam::TextEditorCodeExecutionToolResult(_)
        | claude::ContentBlockParam::ToolSearchToolResult(_)
        | claude::ContentBlockParam::ContainerUpload(_)
        | claude::ContentBlockParam::Compaction(_)
        | claude::ContentBlockParam::Fallback(_) => return Ok(None),
        _future => return Ok(None),
    }))
}

fn content(
    parts: Vec<gemini::Part>,
    role: gemini::ContentRole,
    rest: serde_json::Map<String, serde_json::Value>,
) -> gemini::Content {
    gemini::Content {
        parts,
        role: Some(role),
        rest,
    }
}

fn role(role: claude::MessageRole) -> Result<gemini::ContentRole, TransformError> {
    match role {
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            Ok(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model))
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            Ok(gemini::ContentRole::Known(gemini::ContentRoleKnown::System))
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::User) => {
            Ok(gemini::ContentRole::Known(gemini::ContentRoleKnown::User))
        }
        claude::MessageRole::Unknown(value) => Ok(gemini::ContentRole::Unknown(value)),
        _ => Err(TransformError::unsupported("Claude role", "future role")),
    }
}

fn system_part(block: claude::TextBlock) -> Result<gemini::Part, TransformError> {
    Ok(super::text_part(block.text, block.rest))
}
