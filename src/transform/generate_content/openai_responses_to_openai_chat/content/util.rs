use crate::protocol::openai;

pub(super) fn response_output_to_text(output: openai::ResponseOutput) -> String {
    match output {
        openai::ResponseOutput::Text(text) => text,
        openai::ResponseOutput::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| match part {
                openai::ResponseToolOutputContentPart::InputText { text, .. } => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(super) fn response_detail_to_chat_detail(
    detail: openai::DetailLevel,
) -> Option<openai::ChatImageDetailLevel> {
    match detail {
        openai::DetailLevel::Low => Some(openai::ChatImageDetailLevel::Low),
        openai::DetailLevel::High => Some(openai::ChatImageDetailLevel::High),
        openai::DetailLevel::Auto => Some(openai::ChatImageDetailLevel::Auto),
        openai::DetailLevel::Original => None,
    }
}
