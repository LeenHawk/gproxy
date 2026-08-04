use crate::protocol::openai;

pub(super) fn response_file_to_chat_part(
    file_data: Option<String>,
    file_id: Option<String>,
    file_url: Option<String>,
    filename: Option<String>,
    prompt_cache_breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> openai::ChatContentPart {
    if let Some(url) = file_url {
        return openai::ChatContentPart::Text {
            text: format!("Attachment URL: {url}"),
            prompt_cache_breakpoint,
            extra: Default::default(),
        };
    }
    openai::ChatContentPart::File {
        file: crate::protocol::wire!(openai::ChatFileRef {
            file_data,
            file_id,
            filename,
            extra: Default::default(),
        }),
        prompt_cache_breakpoint,
        extra: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_url_becomes_attachment_text() {
        let part = super::response_file_to_chat_part(
            None,
            None,
            Some("https://files.example/report.pdf".into()),
            None,
            None,
        );
        let value = serde_json::to_value(part).unwrap();
        assert_eq!(value["type"], "text");
        assert_eq!(
            value["text"],
            "Attachment URL: https://files.example/report.pdf"
        );
    }
}
