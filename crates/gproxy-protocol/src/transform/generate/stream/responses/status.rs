use super::*;
pub(super) fn item(
    value: &r::ResponseOutputItem,
    complete_response: bool,
) -> Result<(), TransformError> {
    if let r::ResponseOutputItem::Message(message) = value {
        for part in &message.content {
            if let i::OutputContent::Text(text) = part {
                for token in &text.logprobs {
                    if token.bytes.iter().any(|v| !(0..=255).contains(v))
                        || token
                            .top_logprobs
                            .iter()
                            .any(|v| v.bytes.iter().any(|b| !(0..=255).contains(b)))
                    {
                        return Err(invalid("logprob token bytes outside byte range"));
                    }
                }
            }
        }
    }
    let (nonterminal, incomplete) = match value {
        r::ResponseOutputItem::Message(v) => (
            v.status == i::OutputMessageStatus::InProgress,
            v.status == i::OutputMessageStatus::Incomplete,
        ),
        r::ResponseOutputItem::Reasoning(v) => (
            v.status == Some(i::ReasoningStatus::InProgress),
            v.status == Some(i::ReasoningStatus::Incomplete),
        ),
        r::ResponseOutputItem::FunctionCall(v) => (
            v.status == Some(i::ItemStatus::InProgress),
            v.status == Some(i::ItemStatus::Incomplete),
        ),
        r::ResponseOutputItem::FunctionCallOutput(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::FileSearchCall(v) => (
            matches!(
                v.status,
                i::FileSearchStatus::InProgress | i::FileSearchStatus::Searching
            ),
            v.status == i::FileSearchStatus::Incomplete,
        ),
        r::ResponseOutputItem::WebSearchCall(v) => (
            matches!(
                v.status,
                i::WebSearchStatus::InProgress | i::WebSearchStatus::Searching
            ),
            false,
        ),
        r::ResponseOutputItem::ImageGenerationCall(v) => (
            matches!(
                v.status,
                i::ImageGenerationStatus::InProgress | i::ImageGenerationStatus::Generating
            ),
            false,
        ),
        r::ResponseOutputItem::CodeInterpreterCall(v) => (
            matches!(
                v.status,
                i::CodeInterpreterStatus::InProgress | i::CodeInterpreterStatus::Interpreting
            ),
            v.status == i::CodeInterpreterStatus::Incomplete,
        ),
        r::ResponseOutputItem::McpCall(v) => (
            matches!(
                v.status,
                Some(i::McpCallStatus::InProgress | i::McpCallStatus::Calling)
            ),
            v.status == Some(i::McpCallStatus::Incomplete),
        ),
        r::ResponseOutputItem::ComputerCall(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ComputerCallOutput(v) => (
            v.status == r::ResponseComputerOutputStatus::InProgress,
            v.status == r::ResponseComputerOutputStatus::Incomplete,
        ),
        r::ResponseOutputItem::LocalShellCall(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::LocalShellCallOutput(v) => (
            v.status.flatten() == Some(i::ItemStatus::InProgress),
            v.status.flatten() == Some(i::ItemStatus::Incomplete),
        ),
        r::ResponseOutputItem::ToolSearchCall(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ToolSearchOutput(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ShellCall(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ShellCallOutput(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ApplyPatchCall(v) => {
            (v.status == i::ApplyPatchStatus::InProgress, false)
        }
        r::ResponseOutputItem::CustomToolCallOutput(v) => (
            v.status == i::ItemStatus::InProgress,
            v.status == i::ItemStatus::Incomplete,
        ),
        r::ResponseOutputItem::ProgramOutput(v) => {
            (false, v.status == i::ProgramOutputStatus::Incomplete)
        }
        r::ResponseOutputItem::AdditionalTools(_)
        | r::ResponseOutputItem::Compaction(_)
        | r::ResponseOutputItem::Program(_)
        | r::ResponseOutputItem::ApplyPatchCallOutput(_)
        | r::ResponseOutputItem::McpListTools(_)
        | r::ResponseOutputItem::McpApprovalRequest(_)
        | r::ResponseOutputItem::McpApprovalResponse(_)
        | r::ResponseOutputItem::CustomToolCall(_) => (false, false),
    };
    if nonterminal || complete_response && incomplete {
        Err(invalid(
            "output item status is inconsistent with terminal lifecycle",
        ))
    } else {
        Ok(())
    }
}
