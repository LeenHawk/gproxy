use crate::dispatch::DispatchTable;

/// A named provider instance backed by a channel.
#[derive(Debug, Clone)]
pub struct ProviderDefinition {
    /// Unique instance name (e.g. "openai-prod").
    pub name: String,
    /// Channel kind identifier (e.g. "openai").
    pub channel_kind: String,
    /// Dispatch table (may override channel defaults).
    pub dispatch_table: DispatchTable,
}
