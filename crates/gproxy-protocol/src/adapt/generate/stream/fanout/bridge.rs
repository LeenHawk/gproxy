use super::*;
use crate::transform::generate::{
    chat_responses::stream as hr, claude_chat::stream as ch, claude_gemini::stream as cg,
    gemini_responses::stream as gr,
};

mod sealed {
    pub trait Edge {}
}

/// The four single-result upstream edges that can serve a multi-candidate client.
pub trait FanoutBridge: StreamBridge + sealed::Edge {
    fn reserve(
        &mut self,
        role: IdentityRole,
        ids: &BTreeSet<String>,
        max: usize,
    ) -> Result<(), TransformError>;
    fn fixed_ids(&self) -> BTreeSet<String> {
        BTreeSet::new()
    }
}

macro_rules! edge {
    ($ty:ty) => {
        impl sealed::Edge for $ty {}
        impl FanoutBridge for $ty {
            fn reserve(
                &mut self,
                role: IdentityRole,
                ids: &BTreeSet<String>,
                max: usize,
            ) -> Result<(), TransformError> {
                self.reserve_external_ids(role, ids, max)
            }
        }
    };
}
edge!(ch::ClaudeToChatStream);
edge!(cg::ClaudeToGeminiStream);
edge!(hr::ResponsesToChatStream);

impl sealed::Edge for gr::ResponsesToGeminiStream {}
impl FanoutBridge for gr::ResponsesToGeminiStream {
    fn reserve(
        &mut self,
        role: IdentityRole,
        ids: &BTreeSet<String>,
        max: usize,
    ) -> Result<(), TransformError> {
        self.reserve_external_ids(role, ids, max)
    }
    fn fixed_ids(&self) -> BTreeSet<String> {
        self.reserved_tool_ids().clone()
    }
}
