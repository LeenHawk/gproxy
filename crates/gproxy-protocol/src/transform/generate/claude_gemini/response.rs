use super::usage::ClaudeGeminiUsageFacts;
use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{DeclaredFields, claude::generate_content as c, gemini as g},
};

pub fn claude_to_gemini_response(
    input: c::GenerateContentResponseBody,
    facts: ClaudeGeminiUsageFacts,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
    let input = input.into_declared();
    let mut report = Report::default();
    let mut ids = flow.clone();
    let finish = match input.stop_reason {
        c::StopReason::EndTurn | c::StopReason::StopSequence | c::StopReason::ToolUse => {
            g::FinishReason::Stop
        }
        c::StopReason::MaxTokens | c::StopReason::ModelContextWindowExceeded => {
            g::FinishReason::MaxTokens
        }
        c::StopReason::Refusal => g::FinishReason::Safety,
        c::StopReason::PauseTurn | c::StopReason::Compaction => g::FinishReason::Stop,
    };
    let mut parts = Vec::new();
    let mut citations = Vec::new();
    let mut calls = super::history::Calls::default();
    for block in input.content {
        match block {
            c::ResponseContentBlock::Text(v) => {
                for citation in v.citations.flatten().unwrap_or_default() {
                    match citation {
                        c::ResponseTextCitation::Web(citation) => {
                            citations.push(g::CitationSource::builder().uri(citation.url).build());
                            report.omitted("citation.title/encrypted_index","Gemini preserves source URI but lacks Claude citation title/encrypted location");
                        }
                        c::ResponseTextCitation::Search(citation) => {
                            citations
                                .push(g::CitationSource::builder().uri(citation.source).build());
                            report.omitted(
                                "citation.source_block_indexes",
                                "Gemini source URI has no Claude search block coordinates",
                            );
                        }
                        c::ResponseTextCitation::Char(_)
                        | c::ResponseTextCitation::Page(_)
                        | c::ResponseTextCitation::ContentBlock(_) => report.omitted(
                            "content.citations",
                            "document coordinates require Gemini source URI facts",
                        ),
                    }
                }
                parts.push(g::Part::builder().text(v.text).build());
            }
            c::ResponseContentBlock::Thinking(v) => {
                report.omitted(
                    "content.signature",
                    "native Claude signature requires scoped host state and cannot become a Gemini signature",
                );
                parts.push(g::Part::builder().text(v.thinking).thought(true).build());
            }
            c::ResponseContentBlock::RedactedThinking(_) => report.omitted(
                "content.redacted_thinking",
                "Gemini has no Claude opaque redacted block",
            ),
            c::ResponseContentBlock::ToolUse(v)
                if v.toolset_name.as_ref().is_some_and(Option::is_some) =>
            {
                report.omitted(
                    "toolset_name",
                    "native toolset member has no target definition",
                );
                continue;
            }
            c::ResponseContentBlock::ToolUse(v) => {
                let id = calls.call(
                    Some(v.id),
                    &v.name,
                    crate::Dialect::Claude,
                    &mut ids,
                    policy,
                )?;
                parts.push(
                    g::Part::builder()
                        .function_call(
                            g::FunctionCall::builder(v.name)
                                .args(v.input)
                                .id(id)
                                .build(),
                        )
                        .build(),
                );
            }
            c::ResponseContentBlock::ServerToolUse(_)
            | c::ResponseContentBlock::WebSearchToolResult(_)
            | c::ResponseContentBlock::WebFetchToolResult(_)
            | c::ResponseContentBlock::AdvisorToolResult(_)
            | c::ResponseContentBlock::CodeExecutionToolResult(_)
            | c::ResponseContentBlock::BashCodeExecutionToolResult(_)
            | c::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
            | c::ResponseContentBlock::ToolSearchToolResult(_)
            | c::ResponseContentBlock::McpToolUse(_)
            | c::ResponseContentBlock::McpToolResult(_)
            | c::ResponseContentBlock::ContainerUpload(_)
            | c::ResponseContentBlock::McpToolListing(_)
            | c::ResponseContentBlock::Compaction(_)
            | c::ResponseContentBlock::Fallback(_) => {
                continue;
            }
        }
    }
    let _has_calls = parts.iter().any(|p| p.function_call.is_some());
    let usage = super::usage::to_gemini(input.usage, facts, &mut report)?;
    for (present, field) in [
        (input.container.is_some(), "container"),
        (input.context_management.is_some(), "context_management"),
        (input.diagnostics.is_some(), "diagnostics"),
        (input.stop_details.is_some(), "stop_details"),
        (input.stop_sequence.is_some(), "stop_sequence"),
    ] {
        if present {
            report.omitted(field, "Gemini has no matching Claude response detail");
        }
    }
    if input.model.is_empty() {
        return Err(TransformError::missing_metadata("model"));
    }
    let id = id(&mut ids, policy, crate::Dialect::Claude, Some(input.id))?;
    let mut candidate = g::Candidate::builder()
        .content(
            g::Content::builder()
                .role("model".to_owned())
                .parts(parts)
                .build(),
        )
        .finish_reason(finish)
        .index(0)
        .build();
    if !citations.is_empty() {
        candidate.citation_metadata = Some(
            g::CitationMetadata::builder()
                .citation_sources(citations)
                .build(),
        );
    }
    let out = g::GenerateContentResponseBody::builder()
        .candidates(vec![candidate])
        .usage_metadata(usage)
        .model_version(input.model)
        .response_id(id)
        .build();
    *flow = ids;
    Ok(Converted { value: out, report })
}

pub fn gemini_to_claude_response(
    input: g::GenerateContentResponseBody,
    model_fallback: Option<String>,
    facts: ClaudeGeminiUsageFacts,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    let input = input.into_declared();
    let candidates = input
        .candidates
        .ok_or_else(|| TransformError::invalid_result("candidates", "missing candidate"))?;

    let candidate = candidates.into_iter().next().expect("length checked");
    if candidate.index.is_some_and(|v| v != 0) {
        return Err(TransformError::invalid_result(
            "candidate.index",
            "single candidate index must be zero",
        ));
    }
    if candidate
        .content
        .as_ref()
        .and_then(|v| v.role.as_deref())
        .is_some_and(|v| v != "model")
    {
        return Err(TransformError::invalid_result(
            "candidate.role",
            "model role required",
        ));
    }
    let mut stop = match candidate
        .finish_reason
        .ok_or_else(|| TransformError::missing_metadata("finish_reason"))?
    {
        g::FinishReason::Stop => c::StopReason::EndTurn,
        g::FinishReason::MaxTokens => c::StopReason::MaxTokens,
        g::FinishReason::Safety
        | g::FinishReason::Recitation
        | g::FinishReason::Language
        | g::FinishReason::Blocklist
        | g::FinishReason::ProhibitedContent
        | g::FinishReason::Spii
        | g::FinishReason::ImageSafety
        | g::FinishReason::ImageProhibitedContent
        | g::FinishReason::ImageRecitation => c::StopReason::Refusal,
        g::FinishReason::Unspecified
        | g::FinishReason::Other
        | g::FinishReason::MalformedFunctionCall
        | g::FinishReason::ImageOther
        | g::FinishReason::NoImage
        | g::FinishReason::UnexpectedToolCall
        | g::FinishReason::TooManyToolCalls
        | g::FinishReason::MissingThoughtSignature
        | g::FinishReason::MalformedResponse => {
            return Err(TransformError::invalid_result(
                "finish_reason",
                "unsuccessful generation cannot become Claude success",
            ));
        }
    };
    let mut report = Report::default();
    let mut ids = flow.clone();
    let mut calls = super::history::Calls::default();
    let mut content = Vec::new();
    for p in candidate.content.and_then(|v| v.parts).unwrap_or_default() {
        if p.thought_signature.is_some() {
            report.omitted(
                "thought_signature",
                "native Gemini signature must not become Claude signature",
            );
        }
        if p.part_metadata.is_some() {
            report.omitted("part_metadata", "Claude lacks Gemini part metadata");
        }
        if p.thought == Some(true) && p.text.is_some() {
            report.omitted(
                "thought",
                "Claude thinking requires native signed block from scoped state",
            );
            continue;
        }
        if p.thought == Some(true) {
            report.omitted(
                "thought",
                "non-text payload is preserved without a foreign reasoning flag",
            );
        }
        if let Some(text) = p.text {
            content.push(c::ResponseContentBlock::Text(
                c::ResponseTextBlock::builder(c::ResponseTextBlockType::Tag, text).build(),
            ));
        }
        if let Some(call) = p.function_call {
            let id = calls.call(
                call.id,
                &call.name,
                crate::Dialect::Gemini,
                &mut ids,
                policy,
            )?;
            content.push(c::ResponseContentBlock::ToolUse(
                c::ResponseToolUseBlock::builder(
                    c::ResponseToolUseBlockType::Tag,
                    id,
                    call.args.unwrap_or_default(),
                    call.name,
                )
                .build(),
            ));
        }
    }
    if stop == c::StopReason::EndTurn
        && content
            .iter()
            .any(|v| matches!(v, c::ResponseContentBlock::ToolUse(_)))
    {
        stop = c::StopReason::ToolUse;
    }
    let usage = super::usage::to_claude(
        input
            .usage_metadata
            .ok_or_else(|| TransformError::missing_metadata("usage_metadata"))?,
        facts,
        &mut report,
    )?;
    for (present, field) in [
        (candidate.safety_ratings.is_some(), "safety_ratings"),
        (candidate.token_count.is_some(), "token_count"),
        (candidate.citation_metadata.is_some(), "citation_metadata"),
        (candidate.grounding_metadata.is_some(), "grounding_metadata"),
        (
            candidate.grounding_attributions.is_some(),
            "grounding_attributions",
        ),
        (
            candidate.url_context_metadata.is_some(),
            "url_context_metadata",
        ),
        (candidate.logprobs_result.is_some(), "logprobs_result"),
        (candidate.avg_logprobs.is_some(), "avg_logprobs"),
        (candidate.finish_message.is_some(), "finish_message"),
        (input.model_status.is_some(), "model_status"),
        (input.prompt_feedback.is_some(), "prompt_feedback"),
    ] {
        if present {
            report.omitted(field, "Claude lacks equivalent Gemini response metadata");
        }
    }
    let model = input
        .model_version
        .or(model_fallback)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| TransformError::missing_metadata("model_version"))?;
    let id = id(&mut ids, policy, crate::Dialect::Gemini, input.response_id)?;
    let out = c::GenerateContentResponseBody::builder(
        c::GenerateContentResponseBodyType::Tag,
        id,
        content,
        model,
        c::ResponseRole::Assistant,
        stop,
        usage,
    )
    .build();
    *flow = ids;
    Ok(Converted { value: out, report })
}

fn id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    dialect: crate::Dialect,
    source: Option<String>,
) -> Result<String, TransformError> {
    if source.as_ref().is_some_and(String::is_empty) {
        return Err(TransformError::invalid_result("response_id", "empty ID"));
    }
    flow.resolve_or_allocate(
        IdentityRole::Response,
        SourceIdentity::new(dialect, source, 0),
        policy,
    )
    .map(|h| h.emitted_id)
    .map_err(|e| TransformError::shape("identity", e.to_string()))
}
