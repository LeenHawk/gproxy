use gproxy_protocol::claude;

use crate::TransformError;

use super::ClaudeCollector;

impl ClaudeCollector {
    pub(super) fn apply_delta(
        &mut self,
        index: u64,
        delta: claude::EventDelta,
    ) -> Result<(), TransformError> {
        match delta {
            claude::EventDelta::Known(delta) => match *delta {
                claude::KnownEventDelta::Text { text, .. } => {
                    if let Some(claude::ResponseContentBlock::Text(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.text.push_str(&text);
                    }
                }
                claude::KnownEventDelta::Thinking { thinking, .. } => {
                    if let Some(claude::ResponseContentBlock::Thinking(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.thinking.push_str(&thinking);
                    }
                }
                claude::KnownEventDelta::Signature { signature, .. } => {
                    if let Some(claude::ResponseContentBlock::Thinking(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.signature.get_or_insert_default().push_str(&signature);
                    }
                }
                claude::KnownEventDelta::InputJson { partial_json, .. } => {
                    self.json.entry(index).or_default().push_str(&partial_json);
                }
                claude::KnownEventDelta::Compaction {
                    content,
                    encrypted_content,
                    ..
                } => {
                    if let Some(claude::ResponseContentBlock::Compaction(block)) =
                        self.blocks.get_mut(&index)
                    {
                        block.content.get_or_insert_default().push_str(&content);
                        block.encrypted_content.push_str(&encrypted_content);
                    }
                }
                claude::KnownEventDelta::Citations { .. } => {}
            },
            claude::EventDelta::Unknown(object) => {
                return Err(TransformError::unsupported(
                    "Claude stream delta",
                    serde_json::to_string(&object)?,
                ));
            }
        }
        Ok(())
    }
}
