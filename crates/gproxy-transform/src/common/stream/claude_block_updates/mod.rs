mod delta;
mod start;
mod stop;

use super::claude_to_responses::ResponseDelta;

pub(super) enum Block {
    Text {
        id: String,
        text: String,
    },
    Thinking {
        id: String,
        text: String,
        signature: Option<String>,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
    },
    Ignored,
}

enum Emission {
    ChatText(String),
    ChatReasoning(String),
    ChatTool(String),
    Responses(ResponseDelta, String, String),
    None,
}
