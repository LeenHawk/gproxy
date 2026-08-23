use std::collections::{BTreeMap, BTreeSet};

use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{functions, media, native, validate};
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
        if !message.rest.is_empty() {
            return Err(TransformError::unsupported(
                "Claude message",
                "message rest",
            ));
        }
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
        for block in blocks {
            parts.push(block_to_part(block, &mut names, &mut native_ids)?);
        }
        if !parts.is_empty() {
            output.push(content(parts, role, Default::default()));
        }
    }
    Ok(output)
}

fn block_to_part(
    block: claude::ContentBlockParam,
    names: &mut BTreeMap<String, String>,
    native_ids: &mut BTreeSet<String>,
) -> Result<gemini::Part, TransformError> {
    Ok(match block {
        claude::ContentBlockParam::Text(block) => {
            validate::text(&block)?;
            super::text_part(block.text, Default::default())
        }
        claude::ContentBlockParam::Thinking(block) => {
            validate::thinking(&block)?;
            functions::thought(block)
        }
        claude::ContentBlockParam::Image(block) => {
            validate::image(&block)?;
            media::image(block.source)?
        }
        claude::ContentBlockParam::Document(block) => {
            validate::document(&block)?;
            media::document(block.source)?
        }
        claude::ContentBlockParam::ToolUse(mut block) if tools::is_native_name(&block.name) => {
            validate::tool_use(&block)?;
            native_ids.insert(block.id.clone());
            let signature = functions::take_signature(&mut block.caller)?;
            let mut part = native::call(block.id, block.input, Default::default())?;
            part.thought_signature = signature;
            part
        }
        claude::ContentBlockParam::ToolUse(mut block) => {
            validate::tool_use(&block)?;
            names.insert(block.id.clone(), block.name.clone());
            let signature = functions::take_signature(&mut block.caller)?;
            functions::function_call(block, signature)
        }
        claude::ContentBlockParam::ServerToolUse(block)
            if tools::is_server_native_name(&block.name) =>
        {
            validate::server_tool(&block)?;
            native_ids.insert(block.id.clone());
            native::call(block.id, block.input, Default::default())?
        }
        claude::ContentBlockParam::ToolResult(block) if native_ids.contains(&block.tool_use_id) => {
            validate::tool_result(&block)?;
            native::result(block)?
        }
        claude::ContentBlockParam::ToolResult(block) => {
            validate::tool_result(&block)?;
            functions::function_result(block, names)?
        }
        claude::ContentBlockParam::BashCodeExecutionToolResult(block) => {
            validate::bash_result(&block)?;
            native::request_bash_result(block)?
        }
        claude::ContentBlockParam::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude raw block",
                raw.to_string(),
            ));
        }
        other => {
            return Err(TransformError::unsupported(
                "Claude content block",
                serde_json::to_string(&other)?,
            ));
        }
    })
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
    if block.cache_control.is_some() || block.citations.is_some() || !block.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Claude system block",
            "cache, citations, or rest",
        ));
    }
    Ok(super::text_part(block.text, Default::default()))
}
