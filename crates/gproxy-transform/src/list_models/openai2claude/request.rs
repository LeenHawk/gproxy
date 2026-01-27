use gproxy_protocol::claude::list_models::request::{
    ListModelsHeaders as ClaudeListModelsHeaders, ListModelsQuery as ClaudeListModelsQuery,
    ListModelsRequest as ClaudeListModelsRequest,
};
use gproxy_protocol::openai::list_models::request::ListModelsRequest as OpenAIListModelsRequest;

/// Convert an OpenAI list-models request into a Claude list-models request.
pub fn transform_request(_request: OpenAIListModelsRequest) -> ClaudeListModelsRequest {
    ClaudeListModelsRequest {
        query: ClaudeListModelsQuery::default(),
        headers: ClaudeListModelsHeaders::default(),
    }
}
