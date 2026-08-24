mod from_claude;
mod to_claude;

use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn claude_to_response(
    tool: claude::Tool,
) -> Result<openai::ResponseTool, TransformError> {
    from_claude::convert(tool)
}

pub(crate) fn response_to_claude(
    tool: openai::ResponseTool,
) -> Result<claude::Tool, TransformError> {
    to_claude::convert(tool)
}
