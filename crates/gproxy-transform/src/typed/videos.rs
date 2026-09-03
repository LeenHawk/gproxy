//! Typed OpenAI Video and Gemini Veo pairs.

use gproxy_protocol::{gemini, openai::video as openai_video};

use super::RequestContext;
use crate::TransformError;

pub mod openai_to_gemini {
    use super::*;

    pub fn create_request(
        input: openai_video::CreateVideoRequest,
    ) -> gemini::VeoPredictLongRunningRequest {
        crate::videos::openai_request_typed(input)
    }

    pub fn response(input: gemini::VeoOperation) -> Result<openai_video::Video, TransformError> {
        crate::videos::gemini_response_to_openai_typed(input)
    }
}

pub mod gemini_to_openai {
    use super::*;

    pub fn create_request(
        input: gemini::VeoPredictLongRunningRequest,
        context: RequestContext<'_>,
    ) -> openai_video::CreateVideoRequest {
        crate::videos::gemini_request_typed(input, context.upstream_model)
    }

    pub fn response(input: openai_video::Video) -> gemini::VeoOperation {
        crate::videos::openai_response_to_gemini_typed(input)
    }
}
