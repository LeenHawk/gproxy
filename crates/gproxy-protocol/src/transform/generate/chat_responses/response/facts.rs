use super::ChatUsageSupplement;
use crate::{
    transform::TransformError,
    wire::{
        DeclaredFields,
        openai::responses::{generate as g, input as i, response as r},
    },
};

/// The original Responses request and effective settings retained during request
/// preparation. Required settings must come from that invocation, not guesses
/// made after receiving a Chat completion. A response conversion consumes this
/// value and recursively removes extensions before reusing same-dialect fields.
#[derive(Debug)]
pub struct ResponsesResponseContext {
    pub request: g::GenerateContentRequestBody,
    pub effective_parallel_tool_calls: bool,
    pub effective_tool_choice: i::ToolChoice,
    pub usage: ChatUsageSupplement,
    pub effective_prompt_cache_options: Option<r::ResponsePromptCacheOptions>,
}
impl ResponsesResponseContext {
    pub(super) fn into_response(
        self,
        id: String,
        created_at: i64,
        model: String,
    ) -> Result<r::GenerateContentResponseBody, TransformError> {
        let request = self.request.into_declared();
        let choice = self.effective_tool_choice.into_declared();
        if request
            .parallel_tool_calls
            .flatten()
            .is_some_and(|v| v != self.effective_parallel_tool_calls)
            || request.tool_choice.as_ref().is_some_and(|v| v != &choice)
        {
            return Err(TransformError::shape(
                "response_context",
                "effective settings contradict original request",
            ));
        }
        let conversation = request.conversation.map(|value| {
            value.map(|value| match value {
                i::ConversationParam::Id(id) => i::ResponseConversationParam {
                    id,
                    rest: Default::default(),
                },
                i::ConversationParam::Object(value) => value,
            })
        });
        let cache = self.effective_prompt_cache_options.into_declared();
        if let Some(requested) = request.prompt_cache_options {
            let actual = cache
                .as_ref()
                .ok_or_else(|| TransformError::missing_metadata("response.prompt_cache_options"))?;
            if requested.mode.is_some_and(|v| v != actual.mode)
                || requested.ttl.is_some_and(|v| v != actual.ttl)
            {
                return Err(TransformError::shape(
                    "response.prompt_cache_options",
                    "effective cache settings contradict request",
                ));
            }
        }
        Ok(r::GenerateContentResponseBody {
            id,
            created_at,
            model,
            object: r::ResponseObject::Response,
            error: None,
            incomplete_details: None,
            instructions: request.instructions.flatten().map(i::Input::Text),
            metadata: request.metadata.flatten(),
            output: Vec::new(),
            parallel_tool_calls: self.effective_parallel_tool_calls,
            temperature: request.temperature.flatten(),
            tool_choice: choice,
            tools: request.tools.unwrap_or_default(),
            top_p: request.top_p.flatten(),
            background: request.background,
            completed_at: None,
            conversation,
            max_output_tokens: request.max_output_tokens,
            max_tool_calls: request.max_tool_calls,
            moderation: None,
            output_text: None,
            previous_response_id: request.previous_response_id,
            prompt: request.prompt,
            prompt_cache_key: request.prompt_cache_key,
            prompt_cache_options: cache,
            prompt_cache_retention: request.prompt_cache_retention,
            reasoning: request.reasoning,
            safety_identifier: request.safety_identifier,
            service_tier: None,
            status: None,
            text: request.text,
            top_logprobs: request.top_logprobs,
            truncation: request.truncation,
            usage: None,
            user: request.user,
            rest: Default::default(),
        })
    }
}
