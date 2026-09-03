use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::super::claude_to_openai::{Output, State, empty_chat_delta};
use super::super::claude_to_responses::{function_item, reasoning_item};
use super::super::state::merge;
use super::Block;

impl State {
    pub(in crate::common::stream) fn block_stop(
        &mut self,
        index: u64,
        rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let Some(block) = self.blocks.remove(&index) else {
            return Ok(Vec::new());
        };
        if matches!(self.output, Output::Chat) {
            return (!rest.is_empty())
                .then(|| self.chat_chunk(empty_chat_delta(rest), None, None))
                .transpose()
                .map(|frame| frame.into_iter().collect());
        }
        if let Block::Tool {
            id,
            name,
            arguments,
            rest: block_rest,
        } = &block
            && items::is_buffered_native(name)
        {
            let input: claude::JsonObject = if arguments.is_empty() {
                Default::default()
            } else {
                serde_json::from_str(arguments)?
            };
            let completed_rest = merge(block_rest.clone(), rest.clone());
            let (in_progress, _) = items::claude_call(
                id.clone(),
                input.clone(),
                name.clone(),
                block_rest.clone(),
                openai::ResponseItemLifecycleStatus::InProgress,
            )?;
            let (completed, _) = items::claude_call(
                id.clone(),
                input,
                name.clone(),
                completed_rest,
                openai::ResponseItemLifecycleStatus::Completed,
            )?;
            let in_progress = openai::ResponseItem::Typed(Box::new(in_progress));
            let completed = openai::ResponseItem::Typed(Box::new(completed));
            self.completed.push(completed.clone());
            return Ok(vec![
                self.response_output_item_added(in_progress, index as u32, Default::default())?,
                self.response_output_item_done(completed, index as u32, rest)?,
            ]);
        }
        let item = match block {
            Block::Text {
                id,
                text,
                rest: block_rest,
            } => openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
                openai::ResponseOutputMessageItem {
                    type_: openai::ResponseMessageItemType::Message,
                    id,
                    role: openai::ResponseOutputMessageRole::Assistant,
                    content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                        openai::ResponseOutputText {
                            type_: openai::ResponseOutputTextType::OutputText,
                            annotations: Vec::new(),
                            logprobs: None,
                            text,
                            rest: merge(block_rest, rest.clone()),
                        },
                    )],
                    status: openai::ResponseItemLifecycleStatus::Completed,
                    phase: None,
                    rest: Default::default(),
                },
            )),
            Block::Thinking {
                id,
                text,
                signature,
                rest: block_rest,
            } => reasoning_item(
                id,
                text,
                signature,
                merge(block_rest, rest.clone()),
                openai::ResponseItemLifecycleStatus::Completed,
            ),
            Block::Tool {
                id,
                name,
                arguments,
                rest: block_rest,
            } => function_item(
                id,
                name,
                arguments,
                merge(block_rest, rest.clone()),
                openai::ResponseItemLifecycleStatus::Completed,
            ),
            Block::Ignored => return Ok(Vec::new()),
        };
        self.completed.push(item.clone());
        Ok(vec![self.response_output_item_done(
            item,
            index as u32,
            rest,
        )?])
    }
}
