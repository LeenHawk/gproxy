use super::{
    common::{invalid, limit},
    gemini_to_responses::GeminiToResponsesStream,
};
use crate::{
    transform::{
        TransformError,
        identity::{IdentityRole, OutputItemKind},
    },
    wire::{
        gemini as g,
        openai::responses::{input as i, response as r, stream as s},
    },
};

impl GeminiToResponsesStream {
    pub(super) fn part(
        &mut self,
        part: g::Part,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        let logical = self.part_index as u64;
        self.part_index += 1;

        let carried = super::super::response::next_signature(&part).is_some();
        if let Some(blob) = &part.inline_data {
            super::super::images::requested_format(blob, &self.context.response.request)?;
            let id = super::super::identity::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::Message,
                IdentityRole::OutputItem(OutputItemKind::ImageGenerationCall),
                None,
                logical,
            )?;
            let complete =
                super::super::images::to_responses(blob.clone(), id, self.limits.max_bytes as u64)?;
            let mut added = complete.clone();
            added.result = None;
            added.status = i::ImageGenerationStatus::InProgress;
            if carried {
                self.carrier(logical, out)?;
            }
            let index = self.add_output(r::ResponseOutputItem::ImageGenerationCall(added), out)?;
            self.target.item_done(
                &mut self.budget,
                out,
                index,
                r::ResponseOutputItem::ImageGenerationCall(complete),
            )?;
            return Ok(());
        }
        if let Some(text) = super::super::response::thought_text(&part) {
            let reasoning = part.thought == Some(true);
            let kind = if reasoning {
                OutputItemKind::Reasoning
            } else {
                OutputItemKind::Message
            };
            let id = super::super::identity::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::Message,
                IdentityRole::OutputItem(kind),
                None,
                logical,
            )?;
            let item = if reasoning {
                r::ResponseOutputItem::Reasoning(
                    i::ReasoningItem::builder(
                        i::ReasoningItemType::ReasoningItem,
                        id.clone(),
                        Vec::new(),
                    )
                    .status(i::ReasoningStatus::InProgress)
                    .build(),
                )
            } else {
                r::ResponseOutputItem::Message(
                    i::ResponseOutputMessage::builder(
                        id.clone(),
                        Vec::new(),
                        i::OutputMessageRole::Assistant,
                        i::OutputMessageStatus::InProgress,
                        i::MessageType::Message,
                    )
                    .build(),
                )
            };
            let index = self.add_output(item, out)?;
            self.target
                .part_added(&mut self.budget, out, index, id.clone(), reasoning)?;
            if !text.is_empty() {
                self.target
                    .text(&mut self.budget, out, index, id, text, reasoning)?;
            }
        }
        if let Some(call) = part.function_call {
            if call.name.is_empty()
                || call
                    .id
                    .as_ref()
                    .is_some_and(|id| id.is_empty() || !self.call_ids.insert(id.clone()))
            {
                return Err(invalid(
                    "empty function name or duplicate/empty native call ID",
                ));
            }
            let call_id = super::super::identity::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::ToolCall,
                IdentityRole::ToolCall,
                call.id.clone(),
                logical,
            )?;
            let item_id = super::super::identity::id(
                &mut self.flow,
                &self.policy,
                IdentityRole::ToolCall,
                IdentityRole::OutputItem(self.client_tools.kind(&call.name)),
                call.id,
                logical,
            )?;
            let arguments = serde_json::to_string(&call.args.unwrap_or_default())?;
            if self.client_tools.kind(&call.name) != OutputItemKind::FunctionCall {
                let item = crate::transform::optional(
                    self.client_tools.restore(
                        i::FunctionCall::builder(
                            i::FunctionCallType::FunctionCall,
                            arguments,
                            call_id,
                            call.name,
                        )
                        .id(item_id)
                        .status(i::ItemStatus::InProgress)
                        .build(),
                    ),
                )?;
                if let Some(item) = item {
                    if carried {
                        self.carrier(logical, out)?;
                    }
                    self.add_output(item, out)?;
                }
                return Ok(());
            }
            let item = self.client_tools.restore(
                i::FunctionCall::builder(
                    i::FunctionCallType::FunctionCall,
                    String::new(),
                    call_id,
                    call.name,
                )
                .id(item_id.clone())
                .status(i::ItemStatus::InProgress)
                .build(),
            )?;
            if carried {
                self.carrier(logical, out)?;
            }
            let index = self.add_output(item, out)?;
            self.target
                .arguments(&mut self.budget, out, index, item_id, arguments, false)?;
        }
        Ok(())
    }
    /// The empty reasoning item the collected body places before a signed
    /// call or image; its signature arrives with the item's completion.
    fn carrier(
        &mut self,
        logical: u64,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        let id = super::super::identity::id(
            &mut self.flow,
            &self.policy,
            IdentityRole::Message,
            IdentityRole::OutputItem(OutputItemKind::Reasoning),
            None,
            logical,
        )?;
        self.add_output(
            r::ResponseOutputItem::Reasoning(
                i::ReasoningItem::builder(i::ReasoningItemType::ReasoningItem, id, Vec::new())
                    .status(i::ReasoningStatus::InProgress)
                    .build(),
            ),
            out,
        )?;
        Ok(())
    }
    fn add_output(
        &mut self,
        item: r::ResponseOutputItem,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<i64, TransformError> {
        let index = i64::try_from(self.item_count).map_err(|_| limit())?;
        self.item_count += 1;
        self.target.emit(&mut self.budget, out, |sequence_number| {
            s::StreamEvent::OutputItemAdded(s::OutputItemEvent {
                agent: None,
                sequence_number,
                output_index: index,
                item,
                rest: Default::default(),
            })
        })?;
        Ok(index)
    }
}
