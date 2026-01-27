use gproxy_protocol::claude::list_models::request::ListModelsRequest as ClaudeListModelsRequest;
use gproxy_protocol::openai::list_models::request::ListModelsRequest as OpenAIListModelsRequest;

/// Convert a Claude list-models request into an OpenAI list-models request.
/// Claude pagination and headers are dropped here and should be handled by the provider layer if needed.
pub fn transform_request(_request: ClaudeListModelsRequest) -> OpenAIListModelsRequest {
    OpenAIListModelsRequest
}
