//! Cache policy mapping shared by the direct Claude/OpenAI request pairs.
//! Read usage is independent of retention; these are request cache-write markers.
use crate::{
    transform::Report,
    wire::{
        claude::{content as c, count_tokens::SystemPrompt, generate_content as cg},
        openai::{chat, responses as r},
    },
};

pub(crate) fn claude_to_chat(value: &c::CacheControl) -> chat::PromptCacheBreakpoint {
    let mut marker = chat::PromptCacheBreakpoint::builder(chat::PromptCacheMode::Explicit).build();
    marker.ttl = value.ttl.map(|ttl| match ttl {
        c::CacheTtl::FiveMinutes => chat::PromptCacheBreakpointTtl::ThirtyMinutes,
        c::CacheTtl::OneHour => chat::PromptCacheBreakpointTtl::TwelveHours,
    });
    marker
}

pub(crate) fn chat_to_claude(value: &chat::PromptCacheBreakpoint) -> c::CacheControl {
    let mut cache = c::CacheControl::builder(c::CacheControlType::Ephemeral).build();
    cache.ttl = value.ttl.map(|ttl| match ttl {
        chat::PromptCacheBreakpointTtl::ThirtyMinutes => c::CacheTtl::FiveMinutes,
        chat::PromptCacheBreakpointTtl::TwelveHours => c::CacheTtl::OneHour,
    });
    cache
}

pub(crate) fn claude_to_responses(value: &c::CacheControl) -> r::PromptCacheBreakpoint {
    let mut marker = r::PromptCacheBreakpoint::builder(r::PromptCacheMode::Explicit).build();
    marker.ttl = value.ttl.map(|ttl| match ttl {
        c::CacheTtl::FiveMinutes => r::PromptCacheBreakpointTtl::ThirtyMinutes,
        c::CacheTtl::OneHour => r::PromptCacheBreakpointTtl::TwelveHours,
    });
    marker
}

pub(crate) fn responses_to_claude(value: &r::PromptCacheBreakpoint) -> c::CacheControl {
    let mut cache = c::CacheControl::builder(c::CacheControlType::Ephemeral).build();
    cache.ttl = value.ttl.map(|ttl| match ttl {
        r::PromptCacheBreakpointTtl::ThirtyMinutes => c::CacheTtl::FiveMinutes,
        r::PromptCacheBreakpointTtl::TwelveHours => c::CacheTtl::OneHour,
    });
    cache
}

pub(crate) fn chat_text(value: &c::TextBlock) -> chat::TextPart {
    let mut text = chat::TextPart::builder(chat::TextPartType::Text, value.text.clone()).build();
    text.prompt_cache_breakpoint = value.cache_control.as_ref().map(claude_to_chat);
    text
}

pub(crate) fn claude_text(value: &chat::TextPart) -> c::TextBlock {
    let mut text = c::TextBlock::builder(c::TextBlockType::Tag, value.text.clone()).build();
    text.cache_control = value.prompt_cache_breakpoint.as_ref().map(chat_to_claude);
    text
}

pub(crate) fn responses_text(value: c::TextBlock) -> r::ResponseInputText {
    let mut text =
        r::ResponseInputText::builder(r::ResponseInputTextType::ResponseInputText, value.text)
            .build();
    text.prompt_cache_breakpoint = value.cache_control.as_ref().map(claude_to_responses);
    text
}

pub(crate) fn chat_options(value: &chat::PromptCacheOptions) -> Option<c::CacheControl> {
    if value.mode != Some(chat::PromptCacheOptionMode::Implicit) {
        return None;
    }
    let mut cache = c::CacheControl::builder(c::CacheControlType::Ephemeral).build();
    cache.ttl = value.ttl.map(|_| c::CacheTtl::FiveMinutes);
    Some(cache)
}

pub(crate) fn responses_options(value: &r::PromptCacheOptions) -> Option<c::CacheControl> {
    if value.mode == Some(r::PromptCachingMode::Explicit) {
        return None;
    }
    let mut cache = c::CacheControl::builder(c::CacheControlType::Ephemeral).build();
    cache.ttl = value.ttl.map(|ttl| match ttl {
        r::PromptCacheTtl::ThirtyMinutes => c::CacheTtl::FiveMinutes,
        r::PromptCacheTtl::TwelveHours => c::CacheTtl::OneHour,
    });
    Some(cache)
}

/// Resolve the request default before converting breakpoints to another wire.
/// An explicit per-block lifetime always wins over the request default.
pub(crate) fn responses_breakpoint_defaults(input: &mut r::GenerateContentRequestBody) {
    let Some(ttl) = input.prompt_cache_options.as_ref().and_then(|v| v.ttl) else {
        return;
    };
    let ttl = match ttl {
        r::PromptCacheTtl::ThirtyMinutes => r::PromptCacheBreakpointTtl::ThirtyMinutes,
        r::PromptCacheTtl::TwelveHours => r::PromptCacheBreakpointTtl::TwelveHours,
    };
    let Some(r::Input::Items(items)) = &mut input.input else {
        return;
    };
    for item in items {
        let parts = match item {
            r::InputItem::Message(v) => Some(&mut v.content),
            r::InputItem::Easy(v) => match &mut v.content {
                r::MessageContent::Parts(parts) => Some(parts),
                r::MessageContent::Text(_) => None,
            },
            r::InputItem::CustomToolCallOutput(v) => match &mut v.output {
                r::CustomOutput::Content(parts) => Some(parts),
                r::CustomOutput::Text(_) => None,
            },
            r::InputItem::FunctionCallOutput(v) => {
                if let r::FunctionOutput::Content(parts) = &mut v.output {
                    for part in parts {
                        let marker = match part {
                            r::FunctionOutputContent::Text(v) => &mut v.prompt_cache_breakpoint,
                            r::FunctionOutputContent::Image(v) => &mut v.prompt_cache_breakpoint,
                            r::FunctionOutputContent::File(v) => &mut v.prompt_cache_breakpoint,
                        };
                        if let Some(Some(marker)) = marker {
                            marker.ttl.get_or_insert(ttl);
                        }
                    }
                }
                None
            }
            _ => None,
        };
        for part in parts.into_iter().flatten() {
            let marker = match part {
                r::InputContent::Text(v) => &mut v.prompt_cache_breakpoint,
                r::InputContent::Image(v) => &mut v.prompt_cache_breakpoint,
                r::InputContent::File(v) => &mut v.prompt_cache_breakpoint,
            };
            if let Some(marker) = marker {
                marker.ttl.get_or_insert(ttl);
            }
        }
    }
}

/// Claude automatic caching ends at the last block. Make that boundary explicit
/// before converting, so a 1h policy can become a 12h OpenAI block TTL without
/// moving a cache boundary or changing the lifetime of other breakpoints.
pub(crate) fn prepare_claude(
    input: &mut cg::GenerateContentRequestBody,
    report: &mut Report,
) -> bool {
    let Some(cache) = input.cache_control.take() else {
        return has_claude_markers(input);
    };
    let applied = if let Some(message) = input.messages.last_mut() {
        match &mut message.content {
            c::MessageContent::Text(text) => {
                let mut block =
                    c::TextBlock::builder(c::TextBlockType::Tag, std::mem::take(text)).build();
                block.cache_control = Some(cache);
                message.content = c::MessageContent::Blocks(vec![c::ContentBlock::Text(block)]);
                true
            }
            c::MessageContent::Blocks(blocks) => {
                let slot = match blocks.last_mut() {
                    Some(c::ContentBlock::Text(block)) => Some(&mut block.cache_control),
                    Some(c::ContentBlock::Image(block)) => Some(&mut block.cache_control),
                    Some(c::ContentBlock::Document(block)) => Some(&mut block.cache_control),
                    Some(c::ContentBlock::ToolResult(block)) => Some(&mut block.cache_control),
                    _ => None,
                };
                if let Some(slot) = slot {
                    slot.get_or_insert(cache);
                    true
                } else {
                    false
                }
            }
        }
    } else {
        match &mut input.system {
            Some(SystemPrompt::Text(text)) => {
                let mut block =
                    c::TextBlock::builder(c::TextBlockType::Tag, std::mem::take(text)).build();
                block.cache_control = Some(cache);
                input.system = Some(SystemPrompt::Blocks(vec![block]));
                true
            }
            Some(SystemPrompt::Blocks(blocks)) => {
                if let Some(block) = blocks.last_mut() {
                    block.cache_control.get_or_insert(cache);
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    };
    if applied {
        report.changed(
            "cache_control",
            "automatic cache boundary mapped to an explicit OpenAI breakpoint",
        );
    } else {
        report.omitted(
            "cache_control",
            "the final block has no OpenAI cache breakpoint representation",
        );
    }
    applied || has_claude_markers(input)
}

fn has_claude_markers(input: &cg::GenerateContentRequestBody) -> bool {
    if matches!(&input.system, Some(SystemPrompt::Blocks(blocks)) if blocks.iter().any(|block| block.cache_control.is_some()))
    {
        return true;
    }
    input.messages.iter().any(|message| match &message.content {
        c::MessageContent::Text(_) => false,
        c::MessageContent::Blocks(blocks) => blocks.iter().any(|block| match block {
            c::ContentBlock::Text(block) => block.cache_control.is_some(),
            c::ContentBlock::Image(block) => block.cache_control.is_some(),
            c::ContentBlock::Document(block) => block.cache_control.is_some(),
            c::ContentBlock::ToolResult(block) => block.cache_control.is_some() || matches!(&block.content,
                Some(c::ToolResultContent::Blocks(parts)) if parts.iter().any(|part| match part {
                    c::ToolResultContentBlock::Text(part) => part.cache_control.is_some(),
                    c::ToolResultContentBlock::Image(part) => part.cache_control.is_some(),
                    c::ToolResultContentBlock::Document(part) => part.cache_control.is_some(),
                    c::ToolResultContentBlock::ToolReference(part) => part.cache_control.is_some(),
                    _ => false,
                })),
            _ => false,
        }),
    })
}
