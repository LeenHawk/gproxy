use gproxy_protocol::{claude, openai};

pub(crate) struct ClaudeResult {
    call_id: String,
    content: Option<claude::ToolResultContent>,
    is_error: Option<bool>,
    item_id: Option<String>,
    rest: openai::Rest,
}

pub(crate) fn openai_result(item: openai::TypedResponseItem) -> Option<ClaudeResult> {
    match item {
        openai::TypedResponseItem::ShellCallOutput {
            call_id,
            output,
            id,
            rest,
            ..
        } => {
            let failed = output.iter().any(|part| {
                part.outcome.type_ != "exit"
                    || part
                        .outcome
                        .exit_code
                        .is_some_and(|exit_code| exit_code != 0)
            });
            let text = output
                .into_iter()
                .flat_map(|part| [part.stdout, part.stderr])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Some(ClaudeResult {
                call_id,
                content: (!text.is_empty()).then_some(claude::ToolResultContent::Text(text)),
                is_error: Some(failed),
                item_id: id,
                rest,
            })
        }
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            status,
            id,
            output,
            rest,
            ..
        } => Some(ClaudeResult {
            call_id,
            content: output.map(claude::ToolResultContent::Text),
            is_error: Some(matches!(
                status,
                openai::ResponseApplyPatchCallOutputStatus::Failed
            )),
            item_id: id,
            rest,
        }),
        openai::TypedResponseItem::ProgramOutput {
            id,
            call_id,
            result,
            status,
            rest,
        } => Some(ClaudeResult {
            call_id,
            content: Some(claude::ToolResultContent::Text(result)),
            is_error: Some(!matches!(
                status,
                openai::ResponseItemLifecycleStatus::Completed
            )),
            item_id: Some(id),
            rest,
        }),
        _ => None,
    }
}

pub(crate) fn result_block(mut result: ClaudeResult) -> claude::ContentBlockParam {
    if let Some(item_id) = result.item_id {
        result.rest.insert("openai_item_id".into(), item_id.into());
    }
    claude::ContentBlockParam::ToolResult(claude::ToolResultBlock {
        tool_use_id: result.call_id,
        type_: claude::ToolResultBlockType::ToolResult,
        cache_control: None,
        content: result.content,
        is_error: result.is_error,
        rest: result.rest,
    })
}
