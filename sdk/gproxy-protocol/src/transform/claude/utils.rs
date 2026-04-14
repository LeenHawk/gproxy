use http::StatusCode;

use crate::claude::count_tokens::types as ct;
use crate::claude::count_tokens::types::{
    BetaContentBlockParam, BetaMessageContent, BetaSystemPrompt, Model, ModelKnown,
};
use crate::claude::types::{
    BetaApiError, BetaApiErrorType, BetaAuthenticationError, BetaAuthenticationErrorType,
    BetaBillingError, BetaBillingErrorType, BetaError, BetaErrorResponse, BetaErrorResponseType,
    BetaGatewayTimeoutError, BetaGatewayTimeoutErrorType, BetaInvalidRequestError,
    BetaInvalidRequestErrorType, BetaNotFoundError, BetaNotFoundErrorType, BetaOverloadedError,
    BetaOverloadedErrorType, BetaPermissionError, BetaPermissionErrorType, BetaRateLimitError,
    BetaRateLimitErrorType,
};

pub fn beta_error_response_from_status_message(
    status_code: StatusCode,
    message: String,
) -> BetaErrorResponse {
    let error = match status_code.as_u16() {
        400 | 413 => BetaError::InvalidRequest(BetaInvalidRequestError {
            message,
            type_: BetaInvalidRequestErrorType::InvalidRequestError,
        }),
        401 => BetaError::Authentication(BetaAuthenticationError {
            message,
            type_: BetaAuthenticationErrorType::AuthenticationError,
        }),
        402 => BetaError::Billing(BetaBillingError {
            message,
            type_: BetaBillingErrorType::BillingError,
        }),
        403 => BetaError::Permission(BetaPermissionError {
            message,
            type_: BetaPermissionErrorType::PermissionError,
        }),
        404 => BetaError::NotFound(BetaNotFoundError {
            message,
            type_: BetaNotFoundErrorType::NotFoundError,
        }),
        429 => BetaError::RateLimit(BetaRateLimitError {
            message,
            type_: BetaRateLimitErrorType::RateLimitError,
        }),
        504 => BetaError::GatewayTimeout(BetaGatewayTimeoutError {
            message,
            type_: BetaGatewayTimeoutErrorType::TimeoutError,
        }),
        529 => BetaError::Overloaded(BetaOverloadedError {
            message,
            type_: BetaOverloadedErrorType::OverloadedError,
        }),
        _ => BetaError::Api(BetaApiError {
            message,
            type_: BetaApiErrorType::ApiError,
        }),
    };

    BetaErrorResponse {
        error,
        request_id: String::new(),
        type_: BetaErrorResponseType::Error,
    }
}

pub fn claude_model_to_string(model: &Model) -> String {
    match model {
        Model::Custom(model) => model.clone(),
        Model::Known(model) => match model {
            ModelKnown::ClaudeOpus46 => "claude-opus-4-6",
            ModelKnown::ClaudeOpus4520251101 => "claude-opus-4-5-20251101",
            ModelKnown::ClaudeOpus45 => "claude-opus-4-5",
            ModelKnown::Claude37SonnetLatest => "claude-3-7-sonnet-latest",
            ModelKnown::Claude37Sonnet20250219 => "claude-3-7-sonnet-20250219",
            ModelKnown::Claude35HaikuLatest => "claude-3-5-haiku-latest",
            ModelKnown::Claude35Haiku20241022 => "claude-3-5-haiku-20241022",
            ModelKnown::ClaudeHaiku45 => "claude-haiku-4-5",
            ModelKnown::ClaudeHaiku4520251001 => "claude-haiku-4-5-20251001",
            ModelKnown::ClaudeSonnet420250514 => "claude-sonnet-4-20250514",
            ModelKnown::ClaudeSonnet40 => "claude-sonnet-4-0",
            ModelKnown::Claude4Sonnet20250514 => "claude-4-sonnet-20250514",
            ModelKnown::ClaudeSonnet45 => "claude-sonnet-4-5",
            ModelKnown::ClaudeSonnet4520250929 => "claude-sonnet-4-5-20250929",
            ModelKnown::ClaudeSonnet46 => "claude-sonnet-4-6",
            ModelKnown::ClaudeOpus40 => "claude-opus-4-0",
            ModelKnown::ClaudeOpus420250514 => "claude-opus-4-20250514",
            ModelKnown::Claude4Opus20250514 => "claude-4-opus-20250514",
            ModelKnown::ClaudeOpus4120250805 => "claude-opus-4-1-20250805",
            ModelKnown::Claude3OpusLatest => "claude-3-opus-latest",
            ModelKnown::Claude3Opus20240229 => "claude-3-opus-20240229",
            ModelKnown::Claude3Haiku20240307 => "claude-3-haiku-20240307",
        }
        .to_string(),
    }
}

pub fn beta_message_content_to_text(content: &BetaMessageContent) -> String {
    match content {
        BetaMessageContent::Text(text) => text.clone(),
        BetaMessageContent::Blocks(blocks) => blocks
            .iter()
            .map(beta_content_block_to_text)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub fn beta_system_prompt_to_text(system: Option<BetaSystemPrompt>) -> Option<String> {
    let text = match system {
        Some(BetaSystemPrompt::Text(text)) => text,
        Some(BetaSystemPrompt::Blocks(blocks)) => blocks
            .into_iter()
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n"),
        None => String::new(),
    };

    if text.is_empty() { None } else { Some(text) }
}

fn beta_content_block_to_text(block: &BetaContentBlockParam) -> String {
    match block {
        BetaContentBlockParam::Text(block) => block.text.clone(),
        _ => "[unsupported_content_block]".to_string(),
    }
}

/// Placeholder name used for synthetic `tool_use` blocks injected by
/// [`push_message_block`] when a `tool_result` would otherwise be orphaned
/// (no preceding assistant `tool_use` with a matching `id`). Exposed so
/// tests can refer to it.
pub const ORPHAN_TOOL_USE_PLACEHOLDER_NAME: &str = "tool_use_placeholder";

fn make_placeholder_tool_use(id: String) -> ct::BetaContentBlockParam {
    ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
        id,
        input: ct::JsonObject::new(),
        name: ORPHAN_TOOL_USE_PLACEHOLDER_NAME.to_string(),
        type_: ct::BetaToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
    })
}

fn placeholder_text_block(text: String) -> ct::BetaContentBlockParam {
    ct::BetaContentBlockParam::Text(ct::BetaTextBlockParam {
        text,
        type_: ct::BetaTextBlockType::Text,
        cache_control: None,
        citations: None,
    })
}

fn promote_content_to_blocks(content: &mut ct::BetaMessageContent) {
    if let ct::BetaMessageContent::Text(text) = content {
        let blocks = if text.is_empty() {
            Vec::new()
        } else {
            vec![placeholder_text_block(std::mem::take(text))]
        };
        *content = ct::BetaMessageContent::Blocks(blocks);
    }
}

fn ensure_tool_use_for(messages: &mut Vec<ct::BetaMessageParam>, id: &str) {
    // Locate the assistant message that should pair with this tool_result.
    // If the trailing message is already a user message (for example, an
    // earlier sibling tool_result we just pushed), the assistant message is
    // the one before that; otherwise it is the trailing message itself.
    let trailing_user = matches!(
        messages.last(),
        Some(ct::BetaMessageParam {
            role: ct::BetaMessageRole::User,
            ..
        })
    );
    let assistant_idx = if trailing_user {
        messages.len().checked_sub(2)
    } else {
        messages.len().checked_sub(1)
    };

    let already_paired = matches!(
        assistant_idx.and_then(|j| messages.get(j)),
        Some(ct::BetaMessageParam {
            content: ct::BetaMessageContent::Blocks(blocks),
            role: ct::BetaMessageRole::Assistant,
        }) if blocks.iter().any(|b|
            matches!(b, ct::BetaContentBlockParam::ToolUse(tu) if tu.id == id)
        )
    );
    if already_paired {
        return;
    }

    let placeholder = make_placeholder_tool_use(id.to_string());

    if let Some(j) = assistant_idx
        && matches!(messages[j].role, ct::BetaMessageRole::Assistant)
    {
        promote_content_to_blocks(&mut messages[j].content);
        if let ct::BetaMessageContent::Blocks(blocks) = &mut messages[j].content {
            blocks.push(placeholder);
            return;
        }
    }

    // No suitable assistant slot. Insert a new assistant message before the
    // trailing user message (or at the end if there is none).
    let insert_at = if trailing_user {
        messages.len() - 1
    } else {
        messages.len()
    };
    messages.insert(
        insert_at,
        ct::BetaMessageParam {
            content: ct::BetaMessageContent::Blocks(vec![placeholder]),
            role: ct::BetaMessageRole::Assistant,
        },
    );
}

/// Append a single content block to a Claude `messages` list, building a
/// well-formed conversation as we go:
///
/// * Consecutive blocks for the same role are merged into one message,
///   instead of producing two adjacent same-role messages (which the Claude
///   API rejects).
/// * Whenever a `tool_result` block is appended to a `user` message, we make
///   sure the immediately preceding assistant message contains a matching
///   `tool_use` block. If none exists (for example, when a client uses the
///   OpenAI Responses API with `previous_response_id` and only sends new
///   `function_call_output` items), we synthesize a placeholder `tool_use`
///   so the request still satisfies the API's pairing rule:
///   *"Each `tool_result` block must have a corresponding `tool_use` block
///   in the previous message."*
///
/// Use this helper from every transform that produces Claude messages from a
/// non-Claude source. It centralises the invariants so each converter can
/// stay focused on its own input format.
pub fn push_message_block(
    messages: &mut Vec<ct::BetaMessageParam>,
    role: ct::BetaMessageRole,
    block: ct::BetaContentBlockParam,
) {
    if matches!(role, ct::BetaMessageRole::User)
        && let ct::BetaContentBlockParam::ToolResult(tr) = &block
    {
        ensure_tool_use_for(messages, &tr.tool_use_id);
    }

    if let Some(last) = messages.last_mut()
        && last.role == role
    {
        promote_content_to_blocks(&mut last.content);
        if let ct::BetaMessageContent::Blocks(blocks) = &mut last.content {
            blocks.push(block);
            return;
        }
    }

    messages.push(ct::BetaMessageParam {
        content: ct::BetaMessageContent::Blocks(vec![block]),
        role,
    });
}

#[cfg(test)]
mod push_message_block_tests {
    use super::*;

    fn tool_result_block(id: &str, body: &str) -> ct::BetaContentBlockParam {
        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
            tool_use_id: id.to_string(),
            type_: ct::BetaToolResultBlockType::ToolResult,
            cache_control: None,
            content: Some(ct::BetaToolResultBlockParamContent::Text(body.to_string())),
            is_error: None,
        })
    }

    fn tool_use_block(id: &str, name: &str) -> ct::BetaContentBlockParam {
        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
            id: id.to_string(),
            input: ct::JsonObject::new(),
            name: name.to_string(),
            type_: ct::BetaToolUseBlockType::ToolUse,
            cache_control: None,
            caller: None,
        })
    }

    fn tool_use_ids_in(message: &ct::BetaMessageParam) -> Vec<String> {
        match &message.content {
            ct::BetaMessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ct::BetaContentBlockParam::ToolUse(tu) => Some(tu.id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn tool_result_ids_in(message: &ct::BetaMessageParam) -> Vec<String> {
        match &message.content {
            ct::BetaMessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| match block {
                    ct::BetaContentBlockParam::ToolResult(tr) => Some(tr.tool_use_id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn injects_assistant_message_for_orphaned_tool_result_at_start() {
        // Reproduces the upstream 400: a request whose first message is a
        // user/tool_result with no preceding assistant/tool_use.
        let mut messages = Vec::new();
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_abc", "-0.978"),
        );

        assert_eq!(
            messages.len(),
            2,
            "expected a synthetic assistant prepended"
        );
        assert!(matches!(messages[0].role, ct::BetaMessageRole::Assistant));
        assert!(matches!(messages[1].role, ct::BetaMessageRole::User));
        assert_eq!(tool_use_ids_in(&messages[0]), vec!["toolu_abc"]);
        assert_eq!(tool_result_ids_in(&messages[1]), vec!["toolu_abc"]);
    }

    #[test]
    fn merges_consecutive_tool_results_into_one_user_message() {
        // Exact shape from the bug report: two consecutive tool_result pushes
        // with no matching tool_use ever pushed.
        let mut messages = Vec::new();
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_one", "-0.978"),
        );
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_two", "{...}"),
        );

        // The single placeholder assistant message should pair both ids, and
        // the two tool_results should live in one merged user message.
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, ct::BetaMessageRole::Assistant));
        assert_eq!(
            tool_use_ids_in(&messages[0]),
            vec!["toolu_one".to_string(), "toolu_two".to_string()]
        );
        assert!(matches!(messages[1].role, ct::BetaMessageRole::User));
        assert_eq!(
            tool_result_ids_in(&messages[1]),
            vec!["toolu_one".to_string(), "toolu_two".to_string()]
        );
    }

    #[test]
    fn does_not_inject_when_pair_already_exists() {
        let mut messages = Vec::new();
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::Assistant,
            tool_use_block("toolu_real", "search"),
        );
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_real", "result"),
        );

        assert_eq!(messages.len(), 2);
        assert_eq!(tool_use_ids_in(&messages[0]), vec!["toolu_real"]);
        assert_eq!(
            tool_use_ids_in(&messages[0])
                .iter()
                .filter(|id| id == &"toolu_real")
                .count(),
            1,
            "no duplicate placeholder should have been injected"
        );
    }

    #[test]
    fn appends_to_existing_assistant_text_when_previous_is_text() {
        // The previous message is an assistant text message — we must not
        // insert a second assistant message in a row, so we convert the text
        // content to blocks and append the placeholder tool_use.
        let mut messages = vec![ct::BetaMessageParam {
            content: ct::BetaMessageContent::Text("doing X".to_string()),
            role: ct::BetaMessageRole::Assistant,
        }];
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_xyz", "ok"),
        );

        assert_eq!(messages.len(), 2);
        let assistant = &messages[0];
        assert!(matches!(assistant.role, ct::BetaMessageRole::Assistant));
        let blocks = match &assistant.content {
            ct::BetaMessageContent::Blocks(blocks) => blocks,
            _ => panic!("expected blocks after sanitization"),
        };
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], ct::BetaContentBlockParam::Text(_)));
        match &blocks[1] {
            ct::BetaContentBlockParam::ToolUse(tu) => {
                assert_eq!(tu.id, "toolu_xyz");
                assert_eq!(tu.name, ORPHAN_TOOL_USE_PLACEHOLDER_NAME);
            }
            _ => panic!("expected tool_use placeholder"),
        }
    }

    #[test]
    fn inserts_placeholder_when_previous_message_is_user_text() {
        let mut messages = vec![ct::BetaMessageParam {
            content: ct::BetaMessageContent::Text("context".to_string()),
            role: ct::BetaMessageRole::User,
        }];
        push_message_block(
            &mut messages,
            ct::BetaMessageRole::User,
            tool_result_block("toolu_orphan", "value"),
        );

        for window in messages.windows(2) {
            assert_ne!(
                window[0].role, window[1].role,
                "consecutive same-role messages produced: {messages:#?}"
            );
        }
        let result_pos = messages
            .iter()
            .position(|m| {
                matches!(&m.content, ct::BetaMessageContent::Blocks(blocks)
                    if blocks.iter().any(|b| matches!(b, ct::BetaContentBlockParam::ToolResult(_))))
            })
            .expect("tool_result message");
        assert!(result_pos > 0);
        let prior = &messages[result_pos - 1];
        assert!(matches!(prior.role, ct::BetaMessageRole::Assistant));
        assert_eq!(tool_use_ids_in(prior), vec!["toolu_orphan"]);
    }
}
