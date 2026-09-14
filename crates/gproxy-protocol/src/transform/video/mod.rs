//! Conditional native Sora / OpenRouter video ↔ Gemini Developer API Veo mappings.
//!
//! These are synchronous typed mappings only.  Polling, URL publication and
//! operation state persistence belong to the host adapter; the contexts below
//! retain the facts that a response conversion cannot recover from either
//! wire format.

mod dimensions;
mod options;
mod request;
mod resources;
mod response;

pub use request::{
    PreparedOpenAiRequest, PreparedVeoRequest, ResolvedVideoResource, VeoRequestContext,
    VideoResourceNeed, VideoResourceRole, gemini_to_openai_request, openai_to_gemini_request,
    resource_needs,
};
pub use response::{
    GeminiOperationContext, OpenAiVideoResponseContext, gemini_operation_to_openai_response,
    openai_response_to_gemini_operation,
};

mod native;
pub use native::{
    NativeVideoDefaults, PreparedNativeVeoRequest, PreparedVeoNativeRequest,
    gemini_to_native_request, native_to_gemini_request,
};

mod native_response;
pub use native_response::{
    NativePendingStatus, NativeToVeoContext, NativeVeoResult, NativeVideoResponseFacts,
    gemini_operation_to_native, native_to_gemini_operation,
};
