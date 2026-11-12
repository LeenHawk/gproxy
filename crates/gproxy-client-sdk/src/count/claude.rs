use super::*;
use crate::operations::{CountTokensParams, Params};
use gproxy_protocol::claude::count_tokens::CountTokensRequestBody;

pub(super) fn params(request: &GenerateRequest) -> Result<CountTokensParams> {
    // Claude's renderer insists on an output budget; counting ignores it.
    let mut request = request.clone();
    if request.settings.max_output_tokens.is_none()
        && request.provider_options.claude.max_tokens.is_none()
    {
        request.settings.max_output_tokens = Some(1);
    }
    let body = dialects::claude::render(&request, false)?.request;
    let mut count = CountTokensRequestBody::builder(body.messages, body.model).build();
    count.cache_control = body.cache_control;
    count.context_management = body.context_management;
    count.mcp_servers = body.mcp_servers;
    count.output_config = body.output_config;
    count.output_format = body.output_format;
    count.speed = body.speed;
    count.system = body.system;
    count.thinking = body.thinking;
    count.tool_choice = body.tool_choice;
    count.tools = body.tools;
    Ok(CountTokensParams::Claude(Params::new(
        count,
        Vendor::Claude,
    )))
}
