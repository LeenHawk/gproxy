use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ResponseCreateRequest,
    ctx: &TransformContext,
) -> Result<claude::CreateMessageRequestBody, TransformError> {
    let chat = super::super::openai_responses_to_openai_chat::request(input, ctx)?;
    super::super::openai_chat_to_claude_messages::request(chat, ctx)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{
        ContentGenerationKind, Operation, OperationKey, OperationKind, claude, openai,
    };

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey {
                operation: Operation::GenerateContent,
                kind: OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses),
            },
            OperationKey {
                operation: Operation::GenerateContent,
                kind: OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages),
            },
        )
    }

    #[test]
    fn apply_patch_result_reaches_claude_as_tool_result() {
        let input = openai::ResponseCreateRequest {
            model: Some(openai::OpenAiModelId::Unknown("test-model".to_owned())),
            input: Some(openai::ResponseInput::Items(vec![
                serde_json::from_value(json!({
                    "type": "apply_patch_call",
                    "call_id": "call_patch",
                    "operation": {
                        "type": "update_file",
                        "path": "src/lib.rs",
                        "diff": "*** Begin Patch\n*** End Patch"
                    },
                    "status": "completed"
                }))
                .unwrap(),
                serde_json::from_value(json!({
                    "type": "apply_patch_call_output",
                    "call_id": "call_patch",
                    "status": "failed",
                    "output": "Model tried to call unavailable tool 'apply_patch'. Available tools: edit."
                }))
                .unwrap(),
            ])),
            ..Default::default()
        };

        let out = request(input, &ctx()).unwrap();
        assert_eq!(out.messages.len(), 2);

        let claude::MessageParam { content, .. } = &out.messages[0];
        let claude::StringOrArray::Array(blocks) = content else {
            panic!("expected assistant blocks");
        };
        let claude::ContentBlockParam::ToolUse(tool_use) = &blocks[0] else {
            panic!("expected apply_patch tool_use");
        };
        assert_eq!(tool_use.id, "toolu_call_patch");
        assert_eq!(tool_use.name, "apply_patch");
        assert_eq!(
            tool_use.input.get("type").and_then(|v| v.as_str()),
            Some("update_file")
        );

        let claude::MessageParam { content, .. } = &out.messages[1];
        let claude::StringOrArray::Array(blocks) = content else {
            panic!("expected user blocks");
        };
        let claude::ContentBlockParam::ToolResult(result) = &blocks[0] else {
            panic!("expected apply_patch tool_result");
        };
        assert_eq!(result.tool_use_id, "toolu_call_patch");
        assert_eq!(
            result.content,
            Some(claude::ToolResultContent::Text(
                "Model tried to call unavailable tool 'apply_patch'. Available tools: edit."
                    .to_owned()
            ))
        );
    }
}
