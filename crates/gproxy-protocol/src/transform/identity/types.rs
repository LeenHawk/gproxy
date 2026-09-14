use serde::{Deserialize, Serialize};
use std::fmt;

/// A caller-owned 128-bit namespace. Supplying it explicitly makes generated
/// ids deterministic in tests and lets an adapter define invocation scope.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdNamespace(pub [u8; 16]);

impl IdNamespace {
    pub const fn with_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn bytes(self) -> [u8; 16] {
        self.0
    }

    pub fn hex(self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl fmt::Debug for IdNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("IdNamespace").field(&self.hex()).finish()
    }
}

/// Identity is bound to the protocol's stable, serializable dialect enum.
pub type DialectId = crate::Dialect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutputItemKind {
    Message,
    FunctionCall,
    FunctionCallOutput,
    CustomToolCall,
    CustomToolCallOutput,
    ToolSearchCall,
    ToolSearchOutput,
    AdditionalTools,
    Compaction,
    ImageGenerationCall,
    LocalShellCall,
    ShellCall,
    ApplyPatchCall,
    WebSearchCall,
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IdentityRole {
    Response,
    Message,
    OutputItem(OutputItemKind),
    ToolCall,
    Resource,
}

impl IdentityRole {
    pub(crate) fn allocation_key(self) -> &'static str {
        match self {
            Self::Response => "r",
            Self::Message => "m",
            Self::ToolCall => "c",
            Self::Resource => "f",
            Self::OutputItem(kind) => match kind {
                OutputItemKind::Message => "im",
                OutputItemKind::FunctionCall => "if",
                OutputItemKind::FunctionCallOutput => "ifo",
                OutputItemKind::CustomToolCall => "ic",
                OutputItemKind::CustomToolCallOutput => "ico",
                OutputItemKind::Reasoning => "ir",
                OutputItemKind::ToolSearchCall => "it",
                OutputItemKind::ToolSearchOutput => "ito",
                OutputItemKind::AdditionalTools => "ia",
                OutputItemKind::Compaction => "ip",
                OutputItemKind::ImageGenerationCall => "ii",
                OutputItemKind::LocalShellCall => "il",
                OutputItemKind::ShellCall => "ish",
                OutputItemKind::ApplyPatchCall => "iap",
                OutputItemKind::WebSearchCall => "iw",
            },
        }
    }

    pub(crate) fn name(self) -> super::error::IdentityRoleName {
        match self {
            Self::Response => super::error::IdentityRoleName::Response,
            Self::Message => super::error::IdentityRoleName::Message,
            Self::OutputItem(_) => super::error::IdentityRoleName::OutputItem,
            Self::ToolCall => super::error::IdentityRoleName::ToolCall,
            Self::Resource => super::error::IdentityRoleName::Resource,
        }
    }

    pub(crate) fn generated_prefix(self) -> Option<KnownIdPrefix> {
        match self {
            Self::Response => Some(KnownIdPrefix::Response),
            Self::Message => Some(KnownIdPrefix::Message),
            Self::OutputItem(OutputItemKind::Message) => Some(KnownIdPrefix::Message),
            Self::OutputItem(OutputItemKind::FunctionCall) => Some(KnownIdPrefix::FunctionCallItem),
            Self::OutputItem(OutputItemKind::FunctionCallOutput) => None,
            Self::OutputItem(OutputItemKind::CustomToolCall) => Some(KnownIdPrefix::CallToolCall),
            Self::OutputItem(OutputItemKind::CustomToolCallOutput) => None,
            Self::OutputItem(OutputItemKind::ToolSearchCall) => {
                Some(KnownIdPrefix::ToolSearchCallItem)
            }
            Self::OutputItem(OutputItemKind::ToolSearchOutput) => {
                Some(KnownIdPrefix::ToolSearchOutputItem)
            }
            Self::OutputItem(OutputItemKind::AdditionalTools) => {
                Some(KnownIdPrefix::AdditionalToolsItem)
            }
            Self::OutputItem(OutputItemKind::Compaction) => Some(KnownIdPrefix::CompactionItem),
            Self::OutputItem(OutputItemKind::ImageGenerationCall) => {
                Some(KnownIdPrefix::ImageGenerationItem)
            }
            Self::OutputItem(OutputItemKind::LocalShellCall) => Some(KnownIdPrefix::LocalShellItem),
            Self::OutputItem(OutputItemKind::ShellCall | OutputItemKind::ApplyPatchCall) => {
                Some(KnownIdPrefix::ClientToolItem)
            }
            Self::OutputItem(OutputItemKind::WebSearchCall) => Some(KnownIdPrefix::WebSearchItem),
            Self::OutputItem(OutputItemKind::Reasoning) => Some(KnownIdPrefix::Reasoning),
            Self::ToolCall => Some(KnownIdPrefix::Call),
            Self::Resource => None,
        }
    }
}

/// Prefix conventions used when generating ids, not universal input validators.
/// Sources: samples/codex/codex-rs/protocol/src/models.rs `id_prefix()`;
/// upstream_docs OpenAI Responses examples and Claude tool_use examples.
/// Existing legal ids are not rejected just for lacking these prefixes.
/// Exact prefixes used by known wire ids. Matching is boundary-aware:
/// `call_foo` is recognized while `callback` is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KnownIdPrefix {
    /// Gateway-generated item ID for a bound client action, not an upstream convention.
    ClientToolItem,
    Call,
    FunctionCallItem,
    FunctionCallOutputItem,
    CustomToolCallOutputItem,
    ToolSearchCallItem,
    ToolSearchOutputItem,
    AdditionalToolsItem,
    CompactionItem,
    ImageGenerationItem,
    LocalShellItem,
    WebSearchItem,
    CallToolCall,
    Reasoning,
    Message,
    Response,
    Tool,
}

impl KnownIdPrefix {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientToolItem => "gproxy_tool_item_",
            Self::Call => "call_",
            Self::FunctionCallItem => "fc_",
            Self::FunctionCallOutputItem => "fco_",
            Self::CustomToolCallOutputItem => "ctco_",
            Self::ToolSearchCallItem => "tsc_",
            Self::ToolSearchOutputItem => "tso_",
            Self::AdditionalToolsItem => "at_",
            Self::CompactionItem => "cmp_",
            Self::ImageGenerationItem => "ig_",
            Self::LocalShellItem => "lsh_",
            Self::WebSearchItem => "ws_",

            Self::CallToolCall => "ctc_",
            Self::Reasoning => "rs_",
            Self::Message => "msg_",
            Self::Response => "resp_",
            Self::Tool => "toolu_",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::ClientToolItem,
            Self::Call,
            Self::FunctionCallItem,
            Self::FunctionCallOutputItem,
            Self::CustomToolCallOutputItem,
            Self::ToolSearchCallItem,
            Self::ToolSearchOutputItem,
            Self::AdditionalToolsItem,
            Self::CompactionItem,
            Self::ImageGenerationItem,
            Self::LocalShellItem,
            Self::WebSearchItem,
            Self::CallToolCall,
            Self::Reasoning,
            Self::Message,
            Self::Response,
            Self::Tool,
        ]
        .into_iter()
        .find(|prefix| {
            value
                .strip_prefix(prefix.as_str())
                .is_some_and(|suffix| !suffix.is_empty())
        })
    }
}

/// A source identity. `logical_index` is mandatory even when an upstream id
/// exists: it makes same-name or missing-id repetitions explicit.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub dialect: DialectId,
    pub source_id: Option<String>,
    pub logical_index: u64,
}

impl SourceIdentity {
    pub fn new(dialect: DialectId, source_id: Option<String>, logical_index: u64) -> Self {
        Self {
            dialect,
            source_id: source_id.filter(|id| !id.is_empty()),
            logical_index,
        }
    }
}
