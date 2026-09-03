mod delta;
mod start;
mod stop;

use gproxy_protocol::openai;

use super::claude_to_responses::ResponseDelta;

pub(super) enum Block {
    Text {
        id: String,
        text: String,
        rest: openai::Rest,
    },
    Thinking {
        id: String,
        text: String,
        signature: Option<String>,
        rest: openai::Rest,
    },
    Tool {
        id: String,
        name: String,
        arguments: String,
        rest: openai::Rest,
    },
    Ignored,
}

enum Emission {
    ChatText(String, openai::Rest),
    ChatReasoning(String, openai::Rest),
    ChatTool(String, openai::Rest),
    Responses(ResponseDelta, String, String, openai::Rest),
    None,
}
