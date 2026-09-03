use std::collections::BTreeMap;

use gproxy_protocol::openai;

#[derive(Default)]
pub(crate) struct State {
    pub(super) id: Option<String>,
    pub(super) created_at: Option<u64>,
    pub(super) model: Option<openai::OpenAiModelId>,
    pub(super) service_tier: Option<openai::ServiceTier>,
    pub(super) tools: BTreeMap<String, Tool>,
    pub(super) next_tool: u32,
    pub(super) text: String,
    pub(super) reasoning: String,
    pub(super) refusal: String,
    pub(super) started: bool,
    pub(super) stopped: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolKind {
    Function,
    Custom,
}

pub(super) struct Tool {
    pub(super) index: u32,
    pub(super) output_index: u32,
    pub(super) kind: ToolKind,
    pub(super) call_id: String,
    pub(super) name: String,
    pub(super) data: String,
}

pub(super) struct ToolStart {
    pub(super) source_id: String,
    pub(super) call_id: String,
    pub(super) output_index: u32,
    pub(super) name: String,
    pub(super) kind: ToolKind,
}
