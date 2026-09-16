//! Source-owned response controls for the Guardian client's SSE envelope.
use super::{GuardianDialectRequest, GuardianPreparedRequest};
use crate::{
    transform::TransformError,
    wire::openai::{guardian as s, responses as r},
};
impl GuardianPreparedRequest {
    /// A bounded, typed envelope template for the existing response-pair
    /// contexts. Private Guardian history is deliberately not fabricated into
    /// public Responses input. Use this request in GuardianStreamContext.
    pub fn response_request(&self) -> Result<r::GenerateContentRequestBody, TransformError> {
        let source = self.source();
        let mut body = r::GenerateContentRequestBody::builder()
            .model(source.model.clone())
            .instructions(source.instructions.clone())
            .store(Some(source.store))
            .stream(Some(source.stream))
            .parallel_tool_calls(Some(source.parallel_tool_calls))
            .tool_choice(r::ToolChoice::Mode(if source.tool_choice == "none" {
                r::ToolChoiceMode::None
            } else {
                r::ToolChoiceMode::Auto
            }))
            .build();
        body.max_output_tokens = Some(Some(match self.request() {
            GuardianDialectRequest::Claude(v) => v.body.max_tokens,
            GuardianDialectRequest::Gemini(v) => v
                .body
                .generation_config
                .as_ref()
                .and_then(|v| v.max_output_tokens)
                .expect("prepared output budget"),
            GuardianDialectRequest::OpenAiChat(v) => v
                .body
                .max_completion_tokens
                .flatten()
                .expect("prepared output budget"),
            GuardianDialectRequest::OpenAiResponses(v) => v
                .body
                .max_output_tokens
                .flatten()
                .expect("prepared output budget"),
        }));
        body.prompt_cache_key = source.prompt_cache_key.clone().map(Some);
        if let Some(reasoning) = &source.reasoning {
            let mut config = r::ReasoningConfig::builder().build();
            config.effort = reasoning
                .effort
                .as_deref()
                .filter(|v| *v != "model_defined")
                .and_then(super::responses_effort)
                .map(Some);
            config.context = reasoning.context.as_ref().map(|v| {
                Some(match v {
                    s::ReasoningContext::Auto => r::ReasoningContext::Auto,
                    s::ReasoningContext::CurrentTurn => r::ReasoningContext::CurrentTurn,
                    s::ReasoningContext::AllTurns => r::ReasoningContext::AllTurns,
                })
            });
            config.summary = reasoning.summary.as_ref().map(|v| match v {
                s::ReasoningSummary::None => None,
                s::ReasoningSummary::Auto => Some(r::ReasoningSummary::Auto),
                s::ReasoningSummary::Concise => Some(r::ReasoningSummary::Concise),
                s::ReasoningSummary::Detailed => Some(r::ReasoningSummary::Detailed),
            });
            body.reasoning = Some(Some(config));
        }
        if let Some(text) = &source.text {
            let mut config = r::TextConfig::builder().build();
            config.verbosity = text.verbosity.as_ref().map(|v| {
                Some(match v {
                    s::ClientVerbosity::Low => r::TextVerbosity::Low,
                    s::ClientVerbosity::Medium => r::TextVerbosity::Medium,
                    s::ClientVerbosity::High => r::TextVerbosity::High,
                })
            });
            if text.format.is_some() {
                let (name, schema, strict) = super::review_schema(source)?;
                config.format = Some(r::TextFormat::JsonSchema(
                    r::TextFormatJsonSchema::builder(
                        name,
                        schema.as_object().expect("validated object schema").clone(),
                    )
                    .strict(Some(strict))
                    .build(),
                ));
            }
            body.text = Some(config);
        }
        body.service_tier = source
            .service_tier
            .as_deref()
            .map(|v| {
                Ok(Some(match v {
                    "auto" => r::ServiceTier::Auto,
                    "default" => r::ServiceTier::Default,
                    "flex" => r::ServiceTier::Flex,
                    "scale" => r::ServiceTier::Scale,
                    "priority" => r::ServiceTier::Priority,
                    "fast" => r::ServiceTier::Fast,
                    _ => {
                        return Err(TransformError::unsupported(
                            "guardian.service_tier",
                            "unknown source tier",
                        ));
                    }
                }))
            })
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        Ok(body)
    }
}
