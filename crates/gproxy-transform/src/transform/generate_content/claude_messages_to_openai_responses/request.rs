use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: claude::CreateMessageRequestBody,
    ctx: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    let chat = super::super::claude_messages_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_openai_responses::request(chat, ctx)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn preserves_representable_claude_breakpoints_in_responses() {
        let input = serde_json::from_value(json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 32,
            "system": [{"type": "text", "text": "system", "cache_control": {"type": "ephemeral"}}],
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "stable", "cache_control": {"type": "ephemeral"}},
                    {"type": "text", "text": "question"}
                ]},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "answer", "cache_control": {"type": "ephemeral"}}
                ]}
            ]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );

        let output = serde_json::to_value(request(input, &ctx).unwrap()).unwrap();
        assert_eq!(
            output["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(
            output["input"][1]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert!(
            output["input"][1]["content"][1]
                .get("prompt_cache_breakpoint")
                .is_none()
        );
        assert_eq!(
            output["input"][2]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
    }
}
