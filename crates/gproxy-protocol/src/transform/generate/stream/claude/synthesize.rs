use super::{
    ClaudeStreamCollector, ClaudeStreamLimits,
    collector::{bounded, invalid},
};
use crate::{
    transform::{Converted, TransformError},
    wire::{
        DeclaredFields,
        claude::{generate_content as c, stream as s},
    },
};

/// Synthesize native block lifecycles, retaining every declared content variant.
/// Bound the cleaned input before making copies, then validate each emitted
/// event immediately rather than building an unchecked event log.
pub fn synthesize_claude_stream(
    input: c::GenerateContentResponseBody,
    limits: ClaudeStreamLimits,
) -> Result<Converted<Vec<s::StreamEvent>>, TransformError> {
    let input = input.into_declared();
    bounded(&input, limits.max_json_bytes)?;
    let mut start_model = input.model.clone();
    let mut expected_model = None;
    for block in &input.content {
        if let c::ResponseContentBlock::Fallback(block) = block {
            if block.from.model.is_empty() || block.to.model.is_empty() {
                return Err(invalid("fallback.model", "empty fallback model"));
            }
            if let Some(previous) = &expected_model {
                if previous != &block.from.model {
                    return Err(invalid(
                        "fallback.from.model",
                        "fallback chain is inconsistent",
                    ));
                }
            } else {
                start_model = block.from.model.clone();
            }
            expected_model = Some(block.to.model.clone());
        }
    }
    if expected_model
        .as_ref()
        .is_some_and(|model| model != &input.model)
    {
        return Err(invalid(
            "fallback.to.model",
            "final fallback destination differs from buffered model",
        ));
    }

    let mut sink = Sink {
        collector: ClaudeStreamCollector::new(limits),
        events: Vec::new(),
    };
    sink.emit(s::StreamEvent::MessageStart(Box::new(
        s::MessageStartEvent {
            message: s::StreamMessage {
                type_: input.type_,
                id: input.id,
                container: input.container,
                content: Vec::new(),
                context_management: input.context_management.clone(),
                diagnostics: input.diagnostics,
                model: start_model,
                role: input.role,
                stop_details: None,
                stop_reason: None,
                stop_sequence: None,
                usage: input.usage.clone(),
                rest: Default::default(),
            },
            rest: Default::default(),
        },
    )))?;
    for (index, mut block) in input.content.into_iter().enumerate() {
        let index =
            i64::try_from(index).map_err(|_| invalid("content.index", "unrepresentable index"))?;
        let mut deltas = Vec::new();
        match &mut block {
            c::ResponseContentBlock::Text(b) => {
                deltas.push(s::ContentBlockDelta::Text(s::TextDelta {
                    text: std::mem::take(&mut b.text),
                    rest: Default::default(),
                }));
                if let Some(Some(citations)) = &mut b.citations {
                    for citation in std::mem::take(citations) {
                        deltas.push(s::ContentBlockDelta::Citations(s::CitationsDelta {
                            citation,
                            rest: Default::default(),
                        }));
                    }
                }
            }
            c::ResponseContentBlock::Thinking(b) => {
                deltas.push(s::ContentBlockDelta::Thinking(s::ThinkingDelta {
                    thinking: std::mem::take(&mut b.thinking),
                    rest: Default::default(),
                }));
                deltas.push(s::ContentBlockDelta::Signature(s::SignatureDelta {
                    signature: std::mem::take(&mut b.signature),
                    rest: Default::default(),
                }));
            }
            c::ResponseContentBlock::Compaction(b) => {
                deltas.push(s::ContentBlockDelta::Compaction(s::CompactionDelta {
                    content: b.content.take(),
                    encrypted_content: b.encrypted_content.take(),
                    rest: Default::default(),
                }));
            }
            b => {
                if let Some(input) = super::blocks::input(b) {
                    let json = bounded(input, limits.max_json_bytes)?;
                    let partial_json = String::from_utf8(json.to_vec())
                        .map_err(|e| invalid("tool.input", e.to_string()))?;
                    input.clear();
                    deltas.push(s::ContentBlockDelta::InputJson(s::InputJsonDelta {
                        partial_json,
                        rest: Default::default(),
                    }));
                }
            }
        }
        sink.emit(s::StreamEvent::ContentBlockStart(
            s::ContentBlockStartEvent {
                index,
                content_block: block,
                rest: Default::default(),
            },
        ))?;
        for delta in deltas {
            sink.emit(s::StreamEvent::ContentBlockDelta(
                s::ContentBlockDeltaEvent {
                    index,
                    delta,
                    rest: Default::default(),
                },
            ))?;
        }
        sink.emit(s::StreamEvent::ContentBlockStop(s::ContentBlockStopEvent {
            index,
            rest: Default::default(),
        }))?;
    }
    sink.emit(s::StreamEvent::MessageDelta(Box::new(
        s::MessageDeltaEvent {
            context_management: input.context_management,
            delta: s::MessageDelta {
                container: None,
                stop_details: input.stop_details,
                stop_reason: Some(Some(input.stop_reason)),
                stop_sequence: input.stop_sequence,
                rest: Default::default(),
            },
            usage: s::MessageDeltaUsage {
                cache_creation_input_tokens: input.usage.cache_creation_input_tokens,
                cache_read_input_tokens: input.usage.cache_read_input_tokens,
                fallback_credit: input.usage.fallback_credit,
                input_tokens: Some(Some(input.usage.input_tokens)),
                iterations: input.usage.iterations,
                output_tokens: input.usage.output_tokens,
                output_tokens_details: input.usage.output_tokens_details,
                server_tool_use: input.usage.server_tool_use,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
    )))?;
    sink.emit(s::StreamEvent::MessageStop(s::MessageStopEvent {
        rest: Default::default(),
    }))?;
    sink.collector.finish()?;
    Ok(Converted {
        value: sink.events,
        report: Default::default(),
    })
}

struct Sink {
    collector: ClaudeStreamCollector,
    events: Vec<s::StreamEvent>,
}

impl Sink {
    fn emit(&mut self, event: s::StreamEvent) -> Result<(), TransformError> {
        self.collector.push(event.clone())?;
        self.events.push(event);
        Ok(())
    }
}
