use crate::protocol::{claude, openai};

use super::super::super::common::openai_breakpoint;

pub(super) fn claude_document_to_chat_parts(
    block: claude::DocumentBlock,
) -> Vec<openai::ChatContentPart> {
    let breakpoint = openai_breakpoint(block.cache_control);
    let title = block.title;
    match block.source {
        claude::DocumentSource::File(source) => {
            vec![file_part(None, Some(source.file_id), title, breakpoint)]
        }
        claude::DocumentSource::Base64(source) => vec![file_part(
            Some(format!("data:application/pdf;base64,{}", source.data)),
            None,
            title,
            breakpoint,
        )],
        claude::DocumentSource::Text(source) => {
            vec![text_part(document_text(title, source.data), breakpoint)]
        }
        claude::DocumentSource::Url(source) => vec![text_part(
            format!("Attachment URL: {}", source.url),
            breakpoint,
        )],
        claude::DocumentSource::Content(source) => match source.content {
            claude::ContentSourceContent::Text(text) => {
                vec![text_part(document_text(title, text), breakpoint)]
            }
            claude::ContentSourceContent::Blocks(blocks) => blocks
                .into_iter()
                .filter_map(|block| match block {
                    claude::ContentSourceBlock::Text(block) => {
                        Some(text_part(block.text, breakpoint.clone()))
                    }
                    _ => None,
                })
                .collect(),
        },
        claude::DocumentSource::Raw(_) => Vec::new(),
    }
}

fn file_part(
    file_data: Option<String>,
    file_id: Option<String>,
    filename: Option<String>,
    prompt_cache_breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> openai::ChatContentPart {
    openai::ChatContentPart::File {
        file: openai::ChatFileRef {
            file_data,
            file_id,
            filename,
            extra: Default::default(),
        },
        prompt_cache_breakpoint,
        extra: Default::default(),
    }
}

fn text_part(
    text: String,
    prompt_cache_breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> openai::ChatContentPart {
    openai::ChatContentPart::Text {
        text,
        prompt_cache_breakpoint,
        extra: Default::default(),
    }
}

fn document_text(title: Option<String>, text: String) -> String {
    title
        .filter(|title| !title.is_empty())
        .map(|title| format!("{title}\n{text}"))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn base64_pdf_preserves_media_type() {
        let block = serde_json::from_value(json!({
            "type":"document",
            "source":{"type":"base64","media_type":"application/pdf","data":"cGRm"},
            "title":"report.pdf"
        }))
        .unwrap();
        let parts = super::claude_document_to_chat_parts(block);
        let value = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            value["file"]["file_data"],
            "data:application/pdf;base64,cGRm"
        );
    }
}
