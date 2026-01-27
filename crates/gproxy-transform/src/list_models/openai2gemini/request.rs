use gproxy_protocol::gemini::list_models::request::{
    ListModelsQuery as GeminiListModelsQuery, ListModelsRequest as GeminiListModelsRequest,
};
use gproxy_protocol::openai::list_models::request::ListModelsRequest as OpenAIListModelsRequest;

/// Convert an OpenAI list-models request into a Gemini list-models request.
pub fn transform_request(_request: OpenAIListModelsRequest) -> GeminiListModelsRequest {
    GeminiListModelsRequest {
        query: GeminiListModelsQuery::default(),
    }
}
