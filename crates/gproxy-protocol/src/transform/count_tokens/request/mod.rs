//! Direct counting requests reuse the same pair's content/tool field mappings.
//! No generation request or invented output token budget is constructed.
mod claude_gemini;
mod claude_openai;
mod controls;
mod gemini_openai;
use crate::{
    transform::{TransformError, identity::TargetIdPolicy},
    wire::{gemini as g, openai::count_tokens as o},
};
pub use claude_gemini::{claude_to_gemini, gemini_to_claude};
pub use claude_openai::{claude_to_openai, openai_to_claude};
pub use gemini_openai::{gemini_to_openai, openai_to_gemini};
fn model(value: impl Into<String>) -> Result<String, TransformError> {
    let value = value.into();
    if value.trim().is_empty() {
        Err(TransformError::missing_metadata("target_model"))
    } else {
        Ok(value)
    }
}
fn policy(policy: &TargetIdPolicy, dialect: crate::Dialect) -> Result<(), TransformError> {
    if policy.dialect != dialect {
        Err(TransformError::shape(
            "identity.policy",
            "policy dialect differs from count target",
        ))
    } else {
        Ok(())
    }
}
fn openai_state(input: &o::CountTokensRequestBody) -> Result<(), TransformError> {
    if input
        .conversation
        .as_ref()
        .and_then(Option::as_ref)
        .is_some()
        || input
            .previous_response_id
            .as_ref()
            .and_then(Option::as_ref)
            .is_some()
    {
        return Err(TransformError::missing_metadata(
            "count input requires resolved conversation/previous response history",
        ));
    }
    if input.personality.is_some() || input.truncation == Some(o::Truncation::Auto) {
        return Err(TransformError::unsupported(
            "count controls",
            "personality/automatic truncation requires target-specific host preprocessing",
        ));
    }
    Ok(())
}
fn gemini_input(
    input: g::CountTokensRequestBody,
) -> Result<g::EmbeddedGenerateContentRequest, TransformError> {
    if input.contents.is_some() && input.generate_content_request.is_some() {
        return Err(TransformError::shape(
            "count input",
            "contents and generateContentRequest are mutually exclusive",
        ));
    }
    if let Some(embedded) = input.generate_content_request {
        if embedded.model.trim().is_empty() {
            return Err(TransformError::shape(
                "generate_content_request.model",
                "embedded model required",
            ));
        }
        Ok(embedded)
    } else {
        let contents = input.contents.ok_or_else(|| {
            TransformError::missing_metadata("contents or generate_content_request")
        })?;
        Ok(g::EmbeddedGenerateContentRequest::builder(String::new(), contents).build())
    }
}
fn gemini_target(
    model: String,
    contents: Vec<g::Content>,
    system: Option<g::Content>,
    tools: Option<Vec<g::Tool>>,
    tool_config: Option<g::ToolConfig>,
    config: Option<g::GenerationConfig>,
) -> g::CountTokensRequestBody {
    let resource = if model.starts_with("models/") {
        model
    } else {
        format!("models/{model}")
    };
    let mut body = g::EmbeddedGenerateContentRequest::builder(resource, contents).build();
    body.system_instruction = system;
    body.tools = tools;
    body.tool_config = tool_config;
    body.generation_config = config;
    g::CountTokensRequestBody::builder()
        .generate_content_request(body)
        .build()
}
