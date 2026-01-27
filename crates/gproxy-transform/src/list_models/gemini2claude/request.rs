use gproxy_protocol::claude::list_models::request::{
    ListModelsHeaders as ClaudeListModelsHeaders, ListModelsQuery as ClaudeListModelsQuery,
    ListModelsRequest as ClaudeListModelsRequest,
};
use gproxy_protocol::gemini::list_models::request::ListModelsRequest as GeminiListModelsRequest;

/// Convert a Gemini list-models request into a Claude list-models request.
pub fn transform_request(request: GeminiListModelsRequest) -> ClaudeListModelsRequest {
    ClaudeListModelsRequest {
        // Gemini uses page tokens; Claude uses cursor ids. Map page_token to after_id as a best-effort.
        query: ClaudeListModelsQuery {
            after_id: request.query.page_token,
            before_id: None,
            limit: request.query.page_size,
        },
        headers: ClaudeListModelsHeaders::default(),
    }
}
