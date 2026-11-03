use super::*;
use crate::dialects;
use crate::stream::{BlockId, Event};
pub(crate) struct StreamState;
impl StreamState {
    pub(crate) fn new() -> Self {
        Self
    }
    pub(crate) fn usage(&self) -> Usage {
        Usage::default()
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<Event>> {
        use gproxy_protocol::openai::responses::{response as r, stream as s};
        let event: s::StreamEvent = serde_json::from_str(data).map_err(Error::invalid)?;
        let mut out = Vec::new();
        match event {
            s::StreamEvent::Created(value) => out.push(Event::ResponseMetadata {
                metadata: dialects::openai::response_metadata(&value.response),
            }),
            s::StreamEvent::InProgress(value) => out.push(Event::ResponseMetadata {
                metadata: dialects::openai::response_metadata(&value.response),
            }),
            s::StreamEvent::OutputTextDelta(value) => out.push(Event::TextDelta {
                id: BlockId {
                    item: index(value.output_index)?,
                    part: index(value.content_index)?,
                },
                delta: value.delta,
            }),
            s::StreamEvent::OutputTextAnnotationAdded(value) => {
                let annotation =
                    serde_json::from_value(value.annotation).map_err(Error::invalid)?;
                let citation = dialects::citations::parse_responses(annotation)?;
                {
                    out.push(Event::Citation {
                        id: BlockId {
                            item: index(value.output_index)?,
                            part: index(value.content_index)?,
                        },
                        citation,
                    });
                }
            }
            s::StreamEvent::RefusalDelta(value) => out.push(Event::TextDelta {
                id: BlockId {
                    item: index(value.output_index)?,
                    part: index(value.content_index)?,
                },
                delta: value.delta,
            }),
            s::StreamEvent::ReasoningTextDelta(value) => out.push(Event::ReasoningDelta {
                id: BlockId::item(index(value.output_index)?),
                delta: value.delta,
            }),
            s::StreamEvent::ReasoningSummaryTextDelta(value) => out.push(Event::ReasoningDelta {
                id: BlockId::item(index(value.output_index)?),
                delta: value.delta,
            }),
            s::StreamEvent::OutputItemAdded(value) => {
                let call = match value.item {
                    r::ResponseOutputItem::FunctionCall(call) => {
                        Some((call.call_id, call.name, false))
                    }
                    r::ResponseOutputItem::WebSearchCall(call) => {
                        Some((call.id, "web_search".into(), true))
                    }
                    r::ResponseOutputItem::FileSearchCall(call) => {
                        Some((call.id, "file_search".into(), true))
                    }
                    r::ResponseOutputItem::CodeInterpreterCall(call) => {
                        Some((call.id, "code_interpreter".into(), true))
                    }
                    r::ResponseOutputItem::ComputerCall(call) => {
                        Some((call.call_id, "computer".into(), true))
                    }
                    _ => None,
                };
                if let Some((call_id, name, provider_executed)) = call {
                    out.push(Event::ToolCallStart {
                        id: BlockId::item(index(value.output_index)?),
                        call_id,
                        name,
                        provider_executed,
                    });
                }
            }
            s::StreamEvent::FunctionCallArgumentsDelta(value) => {
                out.push(Event::ToolCallArgsDelta {
                    id: BlockId::item(index(value.output_index)?),
                    delta: value.delta,
                })
            }
            s::StreamEvent::OutputItemDone(value) => {
                let item = index(value.output_index)?;
                for source in crate::dialects::shared::sources::openai_item(&value.item)? {
                    out.push(Event::Source { source });
                }
                for (part, block) in dialects::openai::parse_item(value.item, item)?
                    .into_iter()
                    .enumerate()
                {
                    let id = BlockId {
                        item,
                        part: part as u32,
                    };
                    if matches!(block.content, Content::ToolCall(_)) {
                        out.push(Event::ToolCallEnd { id });
                    }
                    out.push(Event::BlockEnd { id, block });
                }
            }
            s::StreamEvent::Completed(value) => out.extend(responses_finish(&value.response)?),
            s::StreamEvent::Incomplete(value) => out.extend(responses_finish(&value.response)?),
            s::StreamEvent::Failed(value) => out.extend(responses_finish(&value.response)?),
            s::StreamEvent::Error(_) => {
                return Err(Error::from_event(data.to_owned()));
            }
            _ => {}
        }
        Ok(out)
    }
    pub(crate) fn finish(self) -> Result<Vec<Event>> {
        let Self { .. } = self;
        Ok(vec![Event::Finish {
            reason: FinishReason::new(FinishReasonKind::Interrupted, None),
            usage: Usage::default(),
        }])
    }
}

#[cfg(feature = "openai")]
fn responses_finish(
    response: &gproxy_protocol::openai::responses::response::GenerateContentResponseBody,
) -> Result<Vec<Event>> {
    let mut out = vec![Event::ResponseMetadata {
        metadata: dialects::openai::response_metadata(response),
    }];
    match dialects::openai::finish_metadata(response) {
        Ok((reason, usage)) => out.push(Event::Finish { reason, usage }),
        Err(error) => {
            out.push(Event::Error { error });
            out.push(Event::Finish {
                reason: dialects::openai::error_finish_reason(response),
                usage: dialects::openai::parse_usage(response)?,
            });
        }
    }
    Ok(out)
}
