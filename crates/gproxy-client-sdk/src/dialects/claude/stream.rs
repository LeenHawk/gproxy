use super::*;
use crate::dialects;
use crate::stream::{BlockId, Event};
use gproxy_protocol::claude::{generate_content as c, stream as s};
use std::collections::BTreeMap;
pub(crate) struct StreamState {
    blocks: BTreeMap<u32, gproxy_protocol::claude::generate_content::ResponseContentBlock>,
    texts: BTreeMap<u32, Block>,
    args: BTreeMap<u32, String>,
    usage: Option<Usage>,
    reason: Option<gproxy_protocol::claude::generate_content::StopReason>,
}
impl StreamState {
    pub(crate) fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            texts: BTreeMap::new(),
            args: BTreeMap::new(),
            usage: None,
            reason: None,
        }
    }
    pub(crate) fn usage(&self) -> Usage {
        self.usage.clone().unwrap_or_default()
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<Event>> {
        let Self {
            blocks,
            texts,
            args,
            usage,
            reason,
        } = self;

        let event: s::StreamEvent = serde_json::from_str(data).map_err(Error::invalid)?;
        let mut out = Vec::new();
        match event {
            s::StreamEvent::MessageStart(event) => {
                out.push(Event::ResponseMetadata {
                    metadata: ResponseMetadata {
                        id: Some(event.message.id.clone()),
                        model: Some(event.message.model.clone()),
                        timestamp: None,
                    },
                });
                *usage = Some(dialects::claude::parse_usage(&event.message.usage)?);
            }
            s::StreamEvent::ContentBlockStart(event) => {
                start_block(blocks, event, &mut out)?;
            }
            s::StreamEvent::ContentBlockDelta(event) => {
                delta_block(blocks, args, event, &mut out)?;
            }
            s::StreamEvent::ContentBlockStop(event) => {
                stop_block(blocks, texts, args, event, &mut out)?;
            }
            s::StreamEvent::MessageDelta(event) => {
                message_delta(usage, reason, texts, event, &mut out)?;
            }
            s::StreamEvent::MessageStop(_) => {
                if !blocks.is_empty() {
                    return Err(Error::invalid("Claude stopped before block completion"));
                }
                let reason = reason
                    .ok_or_else(|| Error::invalid("Claude stopped without a finish reason"))?;
                let usage = usage
                    .take()
                    .ok_or_else(|| Error::invalid("Claude stopped before message start"))?;
                out.push(Event::Finish {
                    reason: dialects::claude::finish_reason(reason),
                    usage,
                });
            }
            s::StreamEvent::Error(_) => {
                return Err(Error::from_event(data.to_owned()));
            }
            s::StreamEvent::Ping(_) => {}
        }
        Ok(out)
    }
    pub(crate) fn finish(self) -> Result<Vec<Event>> {
        let Self {
            blocks,
            texts,
            args,
            usage,
            reason,
            ..
        } = self;
        let mut out = Vec::new();

        let refused =
            reason == Some(gproxy_protocol::claude::generate_content::StopReason::Refusal);
        for (index, mut block) in texts {
            if let Content::Text(text) = &mut block.content {
                text.refusal = refused;
            }
            out.push(Event::BlockEnd {
                id: BlockId::item(index),
                block,
            });
        }
        for (index, native) in blocks {
            let mut block = dialects::claude::parse_block(native)?;
            if let Content::Text(text) = &mut block.content {
                text.refusal = refused;
            }
            if let (Some(arguments), Content::ToolCall(call)) = (
                args.get(&index).filter(|a| !a.is_empty()),
                &mut block.content,
            ) {
                call.arguments = arguments.clone();
            }
            out.push(Event::BlockEnd {
                id: BlockId::item(index),
                block,
            });
        }
        out.push(Event::Finish {
            reason: FinishReason::new(
                FinishReasonKind::Interrupted,
                reason.and_then(|reason| dialects::claude::finish_reason(reason).raw),
            ),
            usage: usage.unwrap_or_default(),
        });

        Ok(out)
    }
}

fn start_block(
    blocks: &mut BTreeMap<u32, c::ResponseContentBlock>,
    event: s::ContentBlockStartEvent,
    out: &mut Vec<Event>,
) -> Result<()> {
    let index = index(event.index)?;
    let id = BlockId::item(index);
    match &event.content_block {
        c::ResponseContentBlock::ToolUse(value) => out.push(Event::ToolCallStart {
            id,
            call_id: value.id.clone(),
            name: value.name.clone(),
            provider_executed: false,
        }),
        c::ResponseContentBlock::ServerToolUse(value) => out.push(Event::ToolCallStart {
            id,
            call_id: value.id.clone(),
            name: serde_json::from_value(serde_json::to_value(value.name).map_err(Error::invalid)?)
                .map_err(Error::invalid)?,
            provider_executed: true,
        }),
        c::ResponseContentBlock::Text(value) if !value.text.is_empty() => {
            out.push(Event::TextDelta {
                id,
                delta: value.text.clone(),
            })
        }
        c::ResponseContentBlock::Thinking(value) if !value.thinking.is_empty() => {
            out.push(Event::ReasoningDelta {
                id,
                delta: value.thinking.clone(),
            })
        }
        _ => {}
    }
    blocks.insert(index, event.content_block);

    Ok(())
}
fn delta_block(
    blocks: &mut BTreeMap<u32, c::ResponseContentBlock>,
    args: &mut BTreeMap<u32, String>,
    event: s::ContentBlockDeltaEvent,
    out: &mut Vec<Event>,
) -> Result<()> {
    let index = index(event.index)?;
    let id = BlockId::item(index);
    let block = blocks
        .get_mut(&index)
        .ok_or_else(|| Error::invalid("Claude delta before block start"))?;
    match (block, event.delta) {
        (c::ResponseContentBlock::Text(value), s::ContentBlockDelta::Text(delta)) => {
            value.text.push_str(&delta.text);
            out.push(Event::TextDelta {
                id,
                delta: delta.text,
            });
        }
        (c::ResponseContentBlock::Thinking(value), s::ContentBlockDelta::Thinking(delta)) => {
            value.thinking.push_str(&delta.thinking);
            out.push(Event::ReasoningDelta {
                id,
                delta: delta.thinking,
            });
        }
        (c::ResponseContentBlock::Thinking(value), s::ContentBlockDelta::Signature(delta)) => {
            value.signature.push_str(&delta.signature)
        }
        (
            c::ResponseContentBlock::ToolUse(_) | c::ResponseContentBlock::ServerToolUse(_),
            s::ContentBlockDelta::InputJson(delta),
        ) => {
            args.entry(index).or_default().push_str(&delta.partial_json);
            out.push(Event::ToolCallArgsDelta {
                id,
                delta: delta.partial_json,
            });
        }
        (c::ResponseContentBlock::Text(value), s::ContentBlockDelta::Citations(delta)) => {
            value
                .citations
                .get_or_insert_with(|| Some(Vec::new()))
                .get_or_insert_with(Vec::new)
                .push(delta.citation.clone());
            out.push(Event::Citation {
                id,
                citation: dialects::citations::parse_claude(delta.citation)?,
            });
        }
        (c::ResponseContentBlock::Compaction(value), s::ContentBlockDelta::Compaction(delta)) => {
            if let Some(Some(text)) = delta.content {
                value
                    .content
                    .get_or_insert_with(|| Some(String::new()))
                    .get_or_insert_with(String::new)
                    .push_str(&text);
            }
            if let Some(Some(text)) = delta.encrypted_content {
                value
                    .encrypted_content
                    .get_or_insert_with(|| Some(String::new()))
                    .get_or_insert_with(String::new)
                    .push_str(&text);
            }
        }
        _ => {
            return Err(Error::invalid(
                "Claude delta does not match its native block type",
            ));
        }
    }

    Ok(())
}
fn stop_block(
    blocks: &mut BTreeMap<u32, c::ResponseContentBlock>,
    texts: &mut BTreeMap<u32, Block>,
    args: &mut BTreeMap<u32, String>,
    event: s::ContentBlockStopEvent,
    out: &mut Vec<Event>,
) -> Result<()> {
    let index = index(event.index)?;
    let id = BlockId::item(index);
    let mut block = blocks
        .remove(&index)
        .ok_or_else(|| Error::invalid("Claude stop before block start"))?;
    // A tool without arguments streams an empty input_json_delta; its input stays
    // the `{}` from content_block_start.
    if let Some(arguments) = args.remove(&index).filter(|a| !a.is_empty()) {
        match &mut block {
            c::ResponseContentBlock::ToolUse(value) => {
                value.input = serde_json::from_str(&arguments).map_err(Error::invalid)?
            }
            c::ResponseContentBlock::ServerToolUse(value) => {
                value.input = serde_json::from_str(&arguments).map_err(Error::invalid)?
            }
            _ => {
                return Err(Error::invalid("tool arguments belong to a non-tool block"));
            }
        }
    }
    if matches!(
        block,
        c::ResponseContentBlock::ToolUse(_) | c::ResponseContentBlock::ServerToolUse(_)
    ) {
        out.push(Event::ToolCallEnd { id });
    }
    let sources = crate::dialects::shared::sources::claude_block(&block)?;
    for source in sources {
        out.push(Event::Source { source });
    }
    let block = dialects::claude::parse_block(block)?;
    if matches!(block.content, Content::Text(_)) {
        texts.insert(index, block);
    } else {
        out.push(Event::BlockEnd { id, block });
    }

    Ok(())
}
fn message_delta(
    usage: &mut Option<Usage>,
    reason: &mut Option<c::StopReason>,
    texts: &mut BTreeMap<u32, Block>,
    event: Box<s::MessageDeltaEvent>,
    out: &mut Vec<Event>,
) -> Result<()> {
    let current = usage
        .as_mut()
        .ok_or_else(|| Error::invalid("Claude message delta before message start"))?;
    current.output_tokens = Some(dialects::count(event.usage.output_tokens)?);
    if let Some(value) = event.usage.input_tokens {
        current.input_tokens = dialects::optional_count(value)?;
    }
    if let Some(value) = event.usage.cache_read_input_tokens {
        current.cache_read_tokens = dialects::optional_count(value)?;
    }
    if let Some(value) = event.usage.cache_creation_input_tokens {
        current.cache_write_tokens = dialects::optional_count(value)?;
    }
    if let Some(value) = event.usage.output_tokens_details {
        current.reasoning_tokens = dialects::optional_count(value.map(|v| v.thinking_tokens))?;
    }
    if let Some(Some(value)) = event.delta.stop_reason {
        *reason = Some(value);
        for (index, mut block) in std::mem::take(texts) {
            if let Content::Text(text) = &mut block.content {
                text.refusal = value == c::StopReason::Refusal;
            }
            out.push(Event::BlockEnd {
                id: BlockId::item(index),
                block,
            });
        }
    }

    Ok(())
}
