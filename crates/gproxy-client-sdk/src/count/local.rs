//! Counting in process. The text a request carries is read from the portable
//! IR and counted with the tokenizer `gproxy-tokenizer` selects for the model.
//! Media, file references, signatures and encrypted reasoning are not text and
//! are not counted, so a request with images counts low.
use crate::*;
use gproxy_tokenizer::Tokenizer;

use super::TokenCount;

/// The input tokens of `request`, estimated without asking the upstream.
pub fn estimate(request: &GenerateRequest) -> Result<TokenCount> {
    // The tokenizer knows models by their own name, not a gateway's route.
    let model = request.model.id.rsplit('/').next().unwrap_or_default();
    let tokenizer = Tokenizer::for_model(model, None).map_err(Error::invalid)?;
    let mut text = String::new();
    for message in &request.messages {
        for block in &message.content {
            content(&block.content, &mut text);
        }
    }
    for tool in &request.tools {
        push(&mut text, &tool.name);
        push(&mut text, &tool.description);
        push(&mut text, &tool.input_schema.to_string());
    }
    Ok(TokenCount {
        input_tokens: Some(tokenizer.count(&text).map_err(Error::invalid)?),
        provider_metadata: ProviderMetadata::new(),
    })
}

fn push(text: &mut String, part: &str) {
    text.push_str(part);
    text.push('\n');
}

fn content(content: &Content, text: &mut String) {
    match content {
        Content::Text(part) => push(text, &part.text),
        Content::Reasoning(part) => push(text, &part.text),
        Content::Document(part) => document(part, text),
        Content::ToolCall(call) => {
            push(text, &call.name);
            push(text, &call.arguments);
        }
        Content::ToolResult(result) => {
            push(text, &result.name);
            match &result.output {
                ToolOutput::Json(value) | ToolOutput::Unknown(value) => {
                    push(text, &value.to_string())
                }
                ToolOutput::Blocks(blocks) => {
                    for block in blocks {
                        match &block.content {
                            ToolResultContent::Text(part) => push(text, &part.text),
                            ToolResultContent::Document(part) => document(part, text),
                            _ => {}
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn document(document: &Document, text: &mut String) {
    for part in [&document.title, &document.context].into_iter().flatten() {
        push(text, part);
    }
    match &document.source {
        DocumentSource::Text { text: body } => push(text, body),
        DocumentSource::Chunks { chunks } => {
            for chunk in chunks {
                if let ChunkContent::Text(part) = &chunk.content {
                    push(text, &part.text);
                }
            }
        }
        _ => {}
    }
}
