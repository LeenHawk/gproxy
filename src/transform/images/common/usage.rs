use crate::protocol::{gemini, openai};

use super::scalar::{i32_to_u32, i64_to_u32, u32_to_i32};

pub(super) fn openai_usage_to_gemini(input: openai::ImageUsage) -> gemini::UsageMetadata {
    gemini::UsageMetadata {
        prompt_token_count: Some(u32_to_i32(input.input_tokens)),
        cached_content_token_count: None,
        candidates_token_count: Some(u32_to_i32(input.output_tokens)),
        tool_use_prompt_token_count: None,
        thoughts_token_count: None,
        total_token_count: Some(u32_to_i32(input.total_tokens)),
        prompt_tokens_details: openai_token_details_to_gemini(input.input_tokens_details),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: input
            .output_tokens_details
            .map(openai_token_details_to_gemini)
            .unwrap_or_default(),
        tool_use_prompt_tokens_details: Vec::new(),
        extra: Default::default(),
    }
}

fn openai_token_details_to_gemini(
    details: openai::ImageTokenDetails,
) -> Vec<gemini::ModalityTokenCount> {
    let mut values = Vec::new();
    push_modality_tokens(
        &mut values,
        gemini::ModalityKnown::Text,
        details.text_tokens,
    );
    push_modality_tokens(
        &mut values,
        gemini::ModalityKnown::Image,
        details.image_tokens,
    );
    values
}

fn push_modality_tokens(
    values: &mut Vec<gemini::ModalityTokenCount>,
    modality: gemini::ModalityKnown,
    token_count: u32,
) {
    if token_count == 0 {
        return;
    }

    values.push(gemini::ModalityTokenCount {
        modality: Some(gemini::Modality::Known(modality)),
        token_count: Some(i64::from(token_count)),
        extra: Default::default(),
    });
}

pub(super) fn gemini_usage_to_openai(input: gemini::UsageMetadata) -> openai::ImageUsage {
    let prompt_details =
        modality_details(&input.prompt_tokens_details, &input.cache_tokens_details);
    let has_output_details = !input.candidates_tokens_details.is_empty();
    let output_details = modality_details(&input.candidates_tokens_details, &[]);
    let input_tokens = input
        .prompt_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| prompt_details.total());
    let output_tokens = input
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| output_details.total());
    let total_tokens = input.total_token_count.map(i32_to_u32).unwrap_or_else(|| {
        input_tokens
            .saturating_add(output_tokens)
            .saturating_add(
                input
                    .tool_use_prompt_token_count
                    .map(i32_to_u32)
                    .unwrap_or_default(),
            )
            .saturating_add(
                input
                    .thoughts_token_count
                    .map(i32_to_u32)
                    .unwrap_or_default(),
            )
    });

    openai::ImageUsage {
        input_tokens,
        input_tokens_details: prompt_details.into_openai(),
        output_tokens,
        total_tokens,
        output_tokens_details: has_output_details.then(|| output_details.into_openai()),
        extra: Default::default(),
    }
}

#[derive(Default)]
struct ModalityDetails {
    text_tokens: u32,
    image_tokens: u32,
}

impl ModalityDetails {
    fn add_details(&mut self, details: &[gemini::ModalityTokenCount]) {
        for detail in details {
            let token_count = detail.token_count.map(i64_to_u32).unwrap_or_default();
            match detail.modality.as_ref() {
                Some(gemini::Modality::Known(gemini::ModalityKnown::Text)) => {
                    self.text_tokens = self.text_tokens.saturating_add(token_count);
                }
                Some(gemini::Modality::Known(gemini::ModalityKnown::Image)) => {
                    self.image_tokens = self.image_tokens.saturating_add(token_count);
                }
                _ => {}
            }
        }
    }

    fn total(&self) -> u32 {
        self.text_tokens.saturating_add(self.image_tokens)
    }

    fn into_openai(self) -> openai::ImageTokenDetails {
        openai::ImageTokenDetails {
            text_tokens: self.text_tokens,
            image_tokens: self.image_tokens,
            extra: Default::default(),
        }
    }
}

fn modality_details(
    primary: &[gemini::ModalityTokenCount],
    secondary: &[gemini::ModalityTokenCount],
) -> ModalityDetails {
    let mut details = ModalityDetails::default();
    details.add_details(primary);
    details.add_details(secondary);
    details
}
