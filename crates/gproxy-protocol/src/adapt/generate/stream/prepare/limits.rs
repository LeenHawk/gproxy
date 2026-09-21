use super::super::event::EventLimits;

impl From<EventLimits> for crate::transform::generate::claude_chat::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_blocks: value.max_parts,
            max_tools: value.max_tools,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::claude_gemini::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
            max_tools: value.max_tools,
            max_blocks: value.max_parts,
            max_parts: value.max_parts,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::claude_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
            max_items: value.max_items,
            max_parts: value.max_parts,
            max_blocks: value.max_parts,
            max_tools: value.max_tools,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::chat_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_items: value.max_items,
            max_tool_calls: value.max_tools,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::gemini_chat::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_choices: value.max_choices,
            max_tools: value.max_tools,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::gemini_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_events: value.max_events,
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
            max_items: value.max_items,
            max_parts: value.max_parts,
            max_tools: value.max_tools,
        }
    }
}
