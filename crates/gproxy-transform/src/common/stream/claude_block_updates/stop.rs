use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, State};
use super::super::claude_to_responses::{function_item, reasoning_item};
use super::Block;

impl State {
    pub(in crate::common::stream) fn block_stop(
        &mut self,
        index: u64,
    ) -> Result<Vec<Bytes>, TransformError> {
        let Some(block) = self.blocks.remove(&index) else {
            return Ok(Vec::new());
        };
        if matches!(self.output, Output::Chat) {
            return Ok(Vec::new());
        }
        if let Block::Tool {
            id,
            name,
            arguments,
        } = &block
            && items::is_buffered_native(name)
        {
            let input: claude::JsonObject = if arguments.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(arguments)?
            };
            let (in_progress, _) = items::claude_call(
                id.clone(),
                input.clone(),
                name.clone(),
                openai::ResponseItemLifecycleStatus::InProgress,
            )?;
            let (completed, _) = items::claude_call(
                id.clone(),
                input,
                name.clone(),
                openai::ResponseItemLifecycleStatus::Completed,
            )?;
            let in_progress = openai::ResponseItem::Typed(Box::new(in_progress));
            let completed = openai::ResponseItem::Typed(Box::new(completed));
            self.completed.push(completed.clone());
            return Ok(vec![
                self.response_output_item_added(in_progress, index as u32)?,
                self.response_output_item_done(completed, index as u32)?,
            ]);
        }
        let item = match block {
            Block::Text { id, text } => openai::ResponseItem::Message(
                openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
                    type_: openai::ResponseMessageItemType::Message,
                    id,
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                        openai::ResponseOutputText {
                            type_: openai::ResponseOutputTextType::OutputText,
                            annotations: Vec::new(),
                            logprobs: None,
                            text,
                            rest: Default::default(),
                        },
                    )],
                    status: openai::ResponseItemLifecycleStatus::Completed,
                    phase: None,
                    rest: Default::default(),
                }),
            ),
            Block::Thinking {
                id,
                text,
                signature,
            } => reasoning_item(
                id,
                text,
                signature,
                openai::ResponseItemLifecycleStatus::Completed,
            ),
            Block::Tool {
                id,
                name,
                arguments,
            } => function_item(
                id,
                name,
                arguments,
                openai::ResponseItemLifecycleStatus::Completed,
            ),
            Block::Ignored => return Ok(Vec::new()),
        };
        self.completed.push(item.clone());
        Ok(vec![self.response_output_item_done(item, index as u32)?])
    }
}
