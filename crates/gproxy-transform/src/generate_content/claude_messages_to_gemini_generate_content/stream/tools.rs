use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::chunks;
use super::state::{PendingTool, State};

impl State {
    pub(super) fn block_start(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
        rest: gemini::JsonMap,
    ) -> Result<Option<gemini::GenerateContentResponse>, TransformError> {
        if let claude::ResponseContentBlock::ToolUse(mut block) = block {
            block.rest.extend(rest);
            self.tools.insert(
                index,
                PendingTool {
                    block,
                    partial: String::new(),
                },
            );
            return Ok(None);
        }
        let part = super::super::content::response_block(block)?;
        Ok(Some(chunks::candidate(part, None, None, rest)))
    }

    pub(super) fn block_stop(
        &mut self,
        index: u64,
        rest: gemini::JsonMap,
    ) -> Result<Option<gemini::GenerateContentResponse>, TransformError> {
        let Some(mut pending) = self.tools.remove(&index) else {
            return Ok((!rest.is_empty()).then(|| chunks::candidate(None, None, None, rest)));
        };
        if !pending.partial.is_empty() {
            let parsed: claude::JsonObject = serde_json::from_str(&pending.partial)?;
            pending.block.input.extend(parsed);
        }
        pending.block.rest.extend(rest);
        let block = claude::ResponseContentBlock::ToolUse(pending.block);
        let part = super::super::content::response_block(block)?;
        Ok(Some(chunks::candidate(
            part,
            None,
            None,
            Default::default(),
        )))
    }
}
