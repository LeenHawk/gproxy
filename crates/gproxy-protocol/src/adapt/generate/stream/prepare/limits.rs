use super::super::event::EventLimits;

impl From<EventLimits> for crate::transform::generate::claude_chat::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::claude_gemini::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::claude_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::chat_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::gemini_chat::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
        }
    }
}

impl From<EventLimits> for crate::transform::generate::gemini_responses::stream::StreamLimits {
    fn from(value: EventLimits) -> Self {
        Self {
            max_bytes: value.max_bytes,
            max_pending: value.max_pending_bytes,
        }
    }
}
