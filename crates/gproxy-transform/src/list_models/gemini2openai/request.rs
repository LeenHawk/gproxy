use gproxy_protocol::gemini::list_models::request::ListModelsRequest as GeminiListModelsRequest;
use gproxy_protocol::openai::list_models::request::ListModelsRequest as OpenAIListModelsRequest;

/// Convert a Gemini list-models request into an OpenAI list-models request.
/// Gemini pagination parameters are dropped here and should be handled by the provider layer if needed.
pub fn transform_request(_request: GeminiListModelsRequest) -> OpenAIListModelsRequest {
    OpenAIListModelsRequest
}
