//! Explicit video invocation, resource publication and durable job bindings.
mod composed;
mod direct;
mod recovery;
mod resources;
pub use recovery::recover_created_result;
mod state;
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
pub use composed::{openai_to_gemini_create_composed, openai_to_gemini_query_composed};
pub use direct::*;
pub use resources::publish_video_output;
pub use state::{VideoBinding, VideoJobState, VideoOriginalRequest, VideoProgress};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy)]
pub struct VideoLimits {
    pub codec: CodecLimits,
    pub max_resource_facts: usize,
}

mod transport;

mod native;
pub use native::{
    native_content, native_to_gemini_create_composed, native_to_gemini_query_composed,
    recover_native_result,
};

mod reverse;
pub use reverse::*;
