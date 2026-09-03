use gproxy_protocol::openai;

use crate::TransformError;

use super::State;

impl State {
    pub(super) fn response(
        &self,
        status: openai::ResponseStatus,
        incomplete_details: Option<openai::IncompleteDetails>,
        usage: Option<openai::ResponseUsage>,
        output: Vec<openai::ResponseItem>,
    ) -> Result<openai::ResponseObject, TransformError> {
        let output_text = output
            .iter()
            .filter_map(|item| match item {
                openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                    Some(
                        message
                            .content
                            .iter()
                            .filter_map(|part| match part {
                                openai::ResponseMessageOutputContentPart::OutputText(part) => {
                                    Some(part.text.as_str())
                                }
                                openai::ResponseMessageOutputContentPart::Refusal(_)
                                | openai::ResponseMessageOutputContentPart::Unknown(_) => None,
                                #[cfg(not(feature = "exhaustive"))]
                                _ => None,
                            })
                            .collect::<String>(),
                    )
                }
                openai::ResponseItem::Message(
                    openai::ResponseMessageItem::Input(_)
                    | openai::ResponseMessageItem::EasyInput(_)
                    | openai::ResponseMessageItem::Unknown(_),
                )
                | openai::ResponseItem::Typed(_)
                | openai::ResponseItem::Unknown(_) => None,
                #[cfg(not(feature = "exhaustive"))]
                _ => None,
            })
            .collect::<String>();
        Ok(crate::wire!(openai::ResponseObject {
            id: self
                .id
                .clone()
                .ok_or_else(|| TransformError::shape("Gemini stream", "responseId is missing"))?,
            created_at: None,
            background: None,
            completed_at: None,
            conversation: None,
            error: None,
            incomplete_details,
            instructions: None,
            max_output_tokens: None,
            max_tool_calls: None,
            metadata: None,
            model: self.model.clone(),
            moderation: None,
            multi_agent: None,
            object: openai::ResponseObjectType::Response,
            output_text: (!output_text.is_empty()).then_some(output_text),
            output,
            parallel_tool_calls: None,
            prompt: None,
            prompt_cache_key: None,
            prompt_cache_options: None,
            prompt_cache_retention: None,
            previous_response_id: None,
            reasoning: None,
            safety_identifier: None,
            service_tier: None,
            status: Some(status),
            store: None,
            temperature: None,
            text: None,
            tool_choice: None,
            tools: None,
            top_logprobs: None,
            top_p: None,
            truncation: None,
            usage,
            user: None,
            rest: Default::default(),
        }))
    }
}
