use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use tokenizers::Tokenizer;

use crate::formats::claude::count_tokens::CountTokensRequest;
use crate::formats::claude::types::{
    BetaContentBlockParam, BetaTextBlockParam, MessageContent, MessageList, SystemPrompt,
    ToolResultContentBlockParam, ToolResultContentParam,
};

const TOKENIZER_JSON: &str = include_str!("tokenizer.json");
const _TOKENIZER_CONFIG_JSON: &str = include_str!("tokenizer_config.json");

static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();

pub(crate) fn count_tokens_for_request(req: &CountTokensRequest) -> Result<u32> {
    let mut parts = Vec::new();
    if let Some(system) = &req.system {
        collect_system_prompt(&mut parts, system);
    }
    collect_message_list(&mut parts, &req.messages);

    let text = parts.join("\n");
    if text.is_empty() {
        return Ok(0);
    }

    let tokenizer = tokenizer()?;
    let encoding = tokenizer
        .encode(text, true)
        .map_err(|err| anyhow!("tokenizer encode failed: {}", err))?;
    let count = encoding.len();
    u32::try_from(count).map_err(|_| anyhow!("token count overflow"))
}

fn tokenizer() -> Result<&'static Tokenizer> {
    if let Some(tokenizer) = TOKENIZER.get() {
        return Ok(tokenizer);
    }
    let tokenizer = Tokenizer::from_bytes(TOKENIZER_JSON.as_bytes())
        .map_err(|err| anyhow!("tokenizer load failed: {}", err))?;
    let _ = TOKENIZER.set(tokenizer);
    TOKENIZER
        .get()
        .ok_or_else(|| anyhow!("tokenizer initialization failed"))
}

fn collect_system_prompt(parts: &mut Vec<String>, system: &SystemPrompt) {
    match system {
        SystemPrompt::Text(text) => parts.push(text.clone()),
        SystemPrompt::Blocks(blocks) => {
            for block in blocks {
                parts.push(block.text.clone());
            }
        }
    }
}

fn collect_message_list(parts: &mut Vec<String>, messages: &MessageList) {
    for message in &messages.0 {
        collect_message_content(parts, &message.content);
    }
}

fn collect_message_content(parts: &mut Vec<String>, content: &MessageContent) {
    match content {
        MessageContent::Text(text) => parts.push(text.clone()),
        MessageContent::Blocks(blocks) => {
            for block in blocks {
                collect_content_block(parts, block);
            }
        }
    }
}

fn collect_content_block(parts: &mut Vec<String>, block: &BetaContentBlockParam) {
    match block {
        BetaContentBlockParam::Text(block) => parts.push(block.text.clone()),
        BetaContentBlockParam::Thinking(block) => parts.push(block.thinking.clone()),
        BetaContentBlockParam::ToolUse(block) => {
            parts.push(block.name.clone());
            if let Ok(serialized) = serde_json::to_string(&block.input) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::ToolResult(block) => {
            if let Some(content) = &block.content {
                collect_tool_result_content(parts, content);
            }
        }
        BetaContentBlockParam::ToolReference(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::SearchResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::WebSearchToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::WebFetchToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::CodeExecutionToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::BashCodeExecutionToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::TextEditorCodeExecutionToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::ToolSearchToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::MCPToolUse(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::MCPToolResult(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::ContainerUpload(block) => {
            if let Ok(serialized) = serde_json::to_string(&block) {
                parts.push(serialized);
            }
        }
        BetaContentBlockParam::Image(_)
        | BetaContentBlockParam::Document(_)
        | BetaContentBlockParam::RedactedThinking(_)
        | BetaContentBlockParam::ServerToolUse(_) => {}
    }
}

fn collect_tool_result_content(parts: &mut Vec<String>, content: &ToolResultContentParam) {
    match content {
        ToolResultContentParam::Text(text) => parts.push(text.clone()),
        ToolResultContentParam::Blocks(blocks) => {
            for block in blocks {
                collect_tool_result_block(parts, block);
            }
        }
    }
}

fn collect_tool_result_block(parts: &mut Vec<String>, block: &ToolResultContentBlockParam) {
    match block {
        ToolResultContentBlockParam::Text(BetaTextBlockParam { text, .. }) => {
            parts.push(text.clone());
        }
        ToolResultContentBlockParam::Image(_)
        | ToolResultContentBlockParam::SearchResult(_)
        | ToolResultContentBlockParam::Document(_)
        | ToolResultContentBlockParam::ToolReference(_) => {}
    }
}
