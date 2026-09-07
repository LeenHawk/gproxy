use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, stream_logprob, tool_item};
use super::{Item, Scalar, State, ToolKind, content};

impl State {
    pub(super) fn item_added(
        &mut self,
        output_index: u32,
        item: openai::ResponseItem,
    ) -> Result<openai::ResponseStreamEvent, TransformError> {
        emit(openai::KnownResponseStreamEvent::ResponseOutputItemAdded(
            crate::wire!(openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            }),
        ))
    }

    fn item_done(
        &mut self,
        output_index: u32,
        item: openai::ResponseItem,
    ) -> Result<openai::ResponseStreamEvent, TransformError> {
        self.items.insert(output_index, item.clone());
        emit(openai::KnownResponseStreamEvent::ResponseOutputItemDone(
            crate::wire!(openai::ResponseOutputItemEvent {
                item: Box::new(item),
                output_index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            }),
        ))
    }

    pub(super) fn part_event(&mut self, item: &Item) -> openai::ResponseContentPartEvent {
        crate::wire!(openai::ResponseContentPartEvent {
            content_index: 0,
            item_id: item.id.clone(),
            output_index: item.index,
            part: content::part(item),
            sequence_number: Some(self.next_sequence()),
            rest: Default::default(),
        })
    }

    pub(super) fn finish_scalar(
        &mut self,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let Some(item) = self.scalar.take() else {
            return Ok(Vec::new());
        };
        let sequence_number = Some(self.next_sequence());
        let done = match item.kind {
            Scalar::Text => openai::KnownResponseStreamEvent::ResponseOutputTextDone(crate::wire!(
                openai::ResponseOutputTextDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    logprobs: (!item.logprobs.is_empty()).then(|| item
                        .logprobs
                        .iter()
                        .cloned()
                        .map(stream_logprob)
                        .collect()),
                    output_index: item.index,
                    sequence_number,
                    text: item.text.clone(),
                    rest: Default::default(),
                }
            )),
            Scalar::Reasoning => openai::KnownResponseStreamEvent::ResponseReasoningTextDone(
                crate::wire!(openai::ResponseContentTextDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    sequence_number,
                    text: item.text.clone(),
                    rest: Default::default(),
                }),
            ),
            Scalar::Refusal => openai::KnownResponseStreamEvent::ResponseRefusalDone(crate::wire!(
                openai::ResponseRefusalDoneEvent {
                    content_index: 0,
                    item_id: item.id.clone(),
                    output_index: item.index,
                    sequence_number,
                    refusal: item.text.clone(),
                    rest: Default::default(),
                }
            )),
        };
        let mut output = vec![emit(done)?];
        if item.kind != Scalar::Reasoning {
            output.push(emit(
                openai::KnownResponseStreamEvent::ResponseContentPartDone(self.part_event(&item)),
            )?);
        }
        output.push(self.item_done(
            item.index,
            content::item(&item, openai::ResponseItemLifecycleStatus::Completed),
        )?);
        Ok(output)
    }

    pub(super) fn finish_items(
        &mut self,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        if self.items_finished {
            return Ok(Vec::new());
        }
        let mut output = self.finish_scalar()?;
        for (_, tool) in std::mem::take(&mut self.tools) {
            let sequence_number = Some(self.next_sequence());
            let event = match tool.kind {
                ToolKind::Function => {
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(
                        crate::wire!(openai::ResponseFunctionCallArgumentsDoneEvent {
                            arguments: tool.arguments.clone(),
                            item_id: Some(tool.id.clone()),
                            name: Some(tool.name.clone()),
                            output_index: tool.index,
                            sequence_number,
                            rest: Default::default(),
                        }),
                    )
                }
                ToolKind::Custom => {
                    openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(crate::wire!(
                        openai::ResponseCustomToolCallInputDoneEvent {
                            input: tool.arguments.clone(),
                            item_id: tool.id.clone(),
                            output_index: tool.index,
                            sequence_number,
                            rest: Default::default(),
                        }
                    ))
                }
            };
            output.push(emit(event)?);
            output.push(self.item_done(
                tool.index,
                tool_item(&tool, openai::ResponseItemLifecycleStatus::Completed),
            )?);
        }
        self.items_finished = true;
        Ok(output)
    }
}
