use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::chunks;
use super::state::{PendingTool, State};

impl State {
    pub(super) fn block_start(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
    ) -> Result<Option<gemini::GenerateContentResponse>, TransformError> {
        if let claude::ResponseContentBlock::RedactedThinking(block) = block {
            self.pending_signature = Some(block.data.clone());
            let part = super::super::content::signature_part(block.data);
            return Ok(Some(chunks::candidate(Some(part), None, None)));
        }
        if let claude::ResponseContentBlock::ToolUse(block) = block {
            self.tools.insert(
                index,
                PendingTool {
                    block,
                    partial: String::new(),
                },
            );
            return Ok(None);
        }
        let mut part = super::super::content::response_block(block)?;
        if let Some(part) = part.as_mut() {
            super::super::content::attach_signature(part, &mut self.pending_signature);
        }
        Ok(Some(chunks::candidate(part, None, None)))
    }

    pub(super) fn block_stop(
        &mut self,
        index: u64,
    ) -> Result<Option<gemini::GenerateContentResponse>, TransformError> {
        let Some(mut pending) = self.tools.remove(&index) else {
            return Ok(None);
        };
        if !pending.partial.is_empty() {
            let parsed: claude::JsonObject = serde_json::from_str(&pending.partial)?;
            pending.block.input.extend(parsed);
        }
        let block = claude::ResponseContentBlock::ToolUse(pending.block);
        let mut part = super::super::content::response_block(block)?;
        if let Some(part) = part.as_mut() {
            super::super::content::attach_signature(part, &mut self.pending_signature);
        }
        Ok(Some(chunks::candidate(part, None, None)))
    }
}
