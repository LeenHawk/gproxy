use super::*;
use crate::{
    transform::generate::stream::claude::{ClaudeStreamCollector, ClaudeStreamLimits},
    wire::claude::{generate_content as c, stream as s},
};
impl sealed::Event for s::StreamEvent {}
impl NativeEvent for s::StreamEvent {
    type Full = c::GenerateContentResponseBody;
    type Collector = ClaudeStreamCollector;
    const DIALECT: Dialect = Dialect::Claude;
    fn collector(_: IdentityFlow, _: TargetIdPolicy, limits: EventLimits) -> Self::Collector {
        ClaudeStreamCollector::new(ClaudeStreamLimits {
            max_events: limits.max_events,
            max_json_bytes: limits.max_bytes,
            max_text_bytes: limits.max_bytes,
            max_blocks: limits.max_parts,
        })
    }
    fn collect(collector: &mut Self::Collector, event: Self) -> Result<(), TransformError> {
        collector.push(event)
    }
    fn collected(
        collector: Self::Collector,
    ) -> Result<Converted<Collected<Self::Full>>, TransformError> {
        let value = collector.finish()?;
        Ok(Converted {
            value: Collected::native(value.value),
            report: value.report,
        })
    }
    fn event_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::MessageStart(_) => "message_start",
            Self::ContentBlockStart(_) => "content_block_start",
            Self::ContentBlockDelta(_) => "content_block_delta",
            Self::ContentBlockStop(_) => "content_block_stop",
            Self::MessageDelta(_) => "message_delta",
            Self::MessageStop(_) => "message_stop",
            Self::Ping(_) => "ping",
            Self::Error(_) => "error",
        })
    }
    fn is_terminal(&self) -> bool {
        matches!(self, Self::MessageStop(_) | Self::Error(_))
    }
    fn tool_declarations(
        &self,
        _complete_tool_names: bool,
    ) -> Vec<(String, ToolCallKind, Option<String>)> {
        let blocks: Vec<&c::ResponseContentBlock> = match self {
            Self::MessageStart(event) => event.message.content.iter().collect(),
            // Native start blocks use the same declared content enum.
            Self::ContentBlockStart(event) => {
                return match &event.content_block {
                    c::ResponseContentBlock::ToolUse(tool) => vec![(
                        tool.id.clone(),
                        ToolCallKind::Function,
                        Some(tool.name.clone()),
                    )],
                    _ => Vec::new(),
                };
            }
            _ => Vec::new(),
        };
        blocks
            .into_iter()
            .filter_map(|block| match block {
                c::ResponseContentBlock::ToolUse(tool) => Some((
                    tool.id.clone(),
                    ToolCallKind::Function,
                    Some(tool.name.clone()),
                )),
                _ => None,
            })
            .collect()
    }
}
