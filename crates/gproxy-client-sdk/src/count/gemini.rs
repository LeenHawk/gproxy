use super::*;
use crate::operations::{CountTokensParams, Params};
use gproxy_protocol::gemini::count_tokens::CountTokensRequestBody;
use gproxy_protocol::gemini::generation::EmbeddedGenerateContentRequest;

pub(super) fn params(request: &GenerateRequest) -> Result<CountTokensParams> {
    let body = dialects::gemini::render(request)?.request;
    let model = request.model.id.clone();
    // The embedded request names its model as a resource.
    let resource = if model.starts_with("models/") {
        model.clone()
    } else {
        format!("models/{model}")
    };
    let mut embedded = EmbeddedGenerateContentRequest::builder(resource, body.contents).build();
    embedded.system_instruction = body.system_instruction;
    embedded.tools = body.tools;
    embedded.tool_config = body.tool_config;
    embedded.generation_config = body.generation_config;
    embedded.cached_content = body.cached_content;
    let count = CountTokensRequestBody::builder()
        .generate_content_request(embedded)
        .build();
    Ok(CountTokensParams::Gemini {
        model,
        params: Box::new(Params::new(count, Vendor::Gemini)),
    })
}
