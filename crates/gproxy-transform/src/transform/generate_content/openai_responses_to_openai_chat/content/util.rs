use crate::protocol::openai;

pub(super) fn response_output_to_chat_content(
    output: openai::ResponseOutput,
) -> openai::ChatTextContent {
    match output {
        openai::ResponseOutput::Text(text) => openai::ChatTextContent::Text(text),
        openai::ResponseOutput::Parts(parts) => openai::ChatTextContent::Parts(
            parts
                .into_iter()
                .filter_map(|part| match part {
                    openai::ResponseToolOutputContentPart::InputText {
                        text,
                        prompt_cache_breakpoint,
                        ..
                    } => Some(openai::ChatTextContentPart::Text {
                        text,
                        prompt_cache_breakpoint,
                        extra: Default::default(),
                    }),
                    openai::ResponseToolOutputContentPart::InputImage {
                        prompt_cache_breakpoint,
                        ..
                    } => {
                        warn_dropped_tool_output_breakpoint(
                            prompt_cache_breakpoint.as_ref(),
                            "input_image",
                        );
                        None
                    }
                    openai::ResponseToolOutputContentPart::InputFile {
                        prompt_cache_breakpoint,
                        ..
                    } => {
                        warn_dropped_tool_output_breakpoint(
                            prompt_cache_breakpoint.as_ref(),
                            "input_file",
                        );
                        None
                    }
                })
                .collect(),
        ),
    }
}

fn warn_dropped_tool_output_breakpoint(
    breakpoint: Option<&openai::PromptCacheBreakpoint>,
    block_type: &str,
) {
    if breakpoint.is_some() {
        tracing::warn!(
            block_type,
            target = "OpenAI Chat tool output",
            "cache breakpoint dropped during protocol conversion"
        );
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
