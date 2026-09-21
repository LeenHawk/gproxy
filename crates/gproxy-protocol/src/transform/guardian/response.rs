use crate::{
    codec,
    transform::{TransformError, TransformErrorKind},
    wire::{
        claude::generate_content as c,
        gemini as g,
        openai::{chat as o, responses as r},
    },
};
use serde::{Deserialize, Serialize};

/// The exact structured result requested by the Guardian review client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianReviewResult {
    #[serde(default)]
    pub risk_level: Option<GuardianRiskLevel>,
    #[serde(default)]
    pub user_authorization: Option<GuardianUserAuthorization>,
    pub outcome: GuardianOutcome,
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardianRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardianUserAuthorization {
    Unknown,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardianOutcome {
    Allow,
    Deny,
}

/// Classification labels are intentionally the complete native contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardianClassifyResult {
    High,
    Low,
}

fn parse_review(text: String) -> Result<GuardianReviewResult, TransformError> {
    let result: GuardianReviewResult = codec::decode_json(text.as_bytes(), result_limits())
        .map_err(|e| {
            TransformError::new(
                TransformErrorKind::InvalidResult,
                "guardian.review",
                e.to_string(),
            )
        })?;
    Ok(result)
}

fn result_limits() -> codec::CodecLimits {
    codec::CodecLimits {
        max_buffer_bytes: 256 * 1024,
        max_value_bytes: 256 * 1024,
        max_body_bytes: 256 * 1024,
        max_line_bytes: 256 * 1024,
        max_part_bytes: 256 * 1024,
        max_parts: 16,
    }
}

fn parse_classify(text: String) -> Result<GuardianClassifyResult, TransformError> {
    match text.trim() {
        "high" => Ok(GuardianClassifyResult::High),
        "low" => Ok(GuardianClassifyResult::Low),
        _ => Err(TransformError::invalid_result(
            "guardian.classify",
            "expected exactly the native high or low label",
        )),
    }
}

pub fn extract_claude(
    input: c::GenerateContentResponseBody,
    operation: super::GuardianOperation,
) -> Result<GuardianExtraction, TransformError> {
    let text = crate::transform::memory::claude_text(input)?;
    extract_text(text, operation)
}

pub fn extract_gemini(
    input: g::GenerateContentResponseBody,
    operation: super::GuardianOperation,
) -> Result<GuardianExtraction, TransformError> {
    let text = crate::transform::memory::gemini_text(input)?;
    extract_text(text, operation)
}

pub fn extract_chat(
    input: o::GenerateContentResponseBody,
    operation: super::GuardianOperation,
) -> Result<GuardianExtraction, TransformError> {
    let text = crate::transform::memory::chat_text(input)?;
    extract_text(text, operation)
}

pub fn extract_responses(
    input: r::GenerateContentResponseBody,
    operation: super::GuardianOperation,
) -> Result<GuardianExtraction, TransformError> {
    let text = crate::transform::memory::responses_text(input)?;
    extract_text(text, operation)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardianExtraction {
    Review(GuardianReviewResult),
    Classify(GuardianClassifyResult),
}

fn extract_text(
    text: String,
    operation: super::GuardianOperation,
) -> Result<GuardianExtraction, TransformError> {
    match operation {
        super::GuardianOperation::Review => parse_review(text).map(GuardianExtraction::Review),
        super::GuardianOperation::Classify => {
            parse_classify(text).map(GuardianExtraction::Classify)
        }
    }
}
