use super::*;
use crate::operations::{CountTokensParams, Params};
use gproxy_protocol::openai::count_tokens::CountTokensRequestBody;

pub(super) fn params(request: &GenerateRequest) -> Result<CountTokensParams> {
    let body = dialects::openai::render(request, false)?.request;
    let mut count = CountTokensRequestBody::builder().build();
    count.conversation = body.conversation;
    count.input = body.input.map(Some);
    count.instructions = body.instructions;
    count.model = body.model.map(Some);
    count.parallel_tool_calls = body.parallel_tool_calls;
    count.previous_response_id = body.previous_response_id;
    count.reasoning = body.reasoning;
    count.text = body.text.map(Some);
    count.tool_choice = body.tool_choice.map(Some);
    count.tools = body.tools.map(Some);
    Ok(CountTokensParams::Openai(Params::new(
        count,
        Vendor::Openai,
    )))
}
