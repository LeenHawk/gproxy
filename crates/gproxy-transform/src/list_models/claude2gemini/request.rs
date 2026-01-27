use gproxy_protocol::claude::list_models::request::ListModelsRequest as ClaudeListModelsRequest;
use gproxy_protocol::gemini::list_models::request::{
    ListModelsQuery as GeminiListModelsQuery, ListModelsRequest as GeminiListModelsRequest,
};

/// Convert a Claude list-models request into a Gemini list-models request.
pub fn transform_request(request: ClaudeListModelsRequest) -> GeminiListModelsRequest {
    GeminiListModelsRequest {
        // Claude uses cursor ids; Gemini uses page tokens. Map after_id to page_token as a best-effort.
        query: GeminiListModelsQuery {
            page_size: request.query.limit,
            page_token: request.query.after_id,
        },
    }
}
