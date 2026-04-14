use serde::{Deserialize, Serialize};

/// The resolved destination of a model alias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAliasTarget {
    /// The provider name selected by the alias.
    pub provider_name: String,
    /// The provider-specific model identifier selected by the alias.
    pub model_id: String,
}
