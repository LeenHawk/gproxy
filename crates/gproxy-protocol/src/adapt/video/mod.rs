//! Explicit video invocation, resource publication and durable job bindings.

mod composed;
mod direct;
mod native;
mod recovery;
mod resources;
mod reverse;
mod state;
mod transport;

use crate::{
    WireRequest,
    adapt::{JsonInvocation, invoke_empty, invoke_json},
    capability::Upstream,
    codec::CodecLimits,
    transform::{
        Converted, TransformError,
        video::{self, GeminiOperationContext, OpenAiVideoResponseContext, ResolvedVideoResource},
    },
    wire::{DeclaredFields, gemini::video as g, openai::video as o},
};
use std::collections::BTreeMap;

pub use composed::{openai_to_gemini_create_composed, openai_to_gemini_query_composed};
pub use direct::*;
pub use native::{
    native_content, native_to_gemini_create_composed, native_to_gemini_query_composed,
    recover_native_result,
};
pub use recovery::recover_created_result;
pub use resources::publish_video_output;
pub use reverse::*;
pub use state::{
    VIDEO_STATE_TTL, VideoBinding, VideoJobState, VideoOriginalRequest, VideoProgress,
};

#[derive(Debug, Clone, Copy)]
pub struct VideoLimits {
    pub codec: CodecLimits,
}
