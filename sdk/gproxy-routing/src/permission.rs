use serde::{Deserialize, Serialize};

/// A single permission entry for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    /// Stable database identity for admin CRUD and cache synchronization.
    pub id: i64,
    /// The provider this permission applies to, or `None` for all providers.
    pub provider_id: Option<i64>,
    /// The allowed model pattern.
    ///
    /// `*` matches all models, `claude-*` is a prefix match, and any other value is exact.
    pub model_pattern: String,
}

/// A provider-scoped file API permission for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePermissionEntry {
    /// Stable database identity for admin CRUD and cache synchronization.
    pub id: i64,
    /// The provider this file permission grants access to.
    pub provider_id: i64,
}

/// Checks whether a model string matches a permission pattern.
///
/// Supports:
/// - `*` matches all models
/// - `prefix*` matches models starting with `prefix`
/// - `*suffix` matches models ending with `suffix`
/// - exact match otherwise
pub fn pattern_matches(pattern: &str, model: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return model.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return model.ends_with(suffix);
    }
    pattern == model
}

/// Checks whether any permission entry grants access to the given provider and model.
pub fn check_model_permission(
    permissions: &[PermissionEntry],
    provider_id: Option<i64>,
    model: &str,
) -> bool {
    permissions.iter().any(|permission| {
        let provider_matches =
            permission.provider_id.is_none() || permission.provider_id == provider_id;
        provider_matches && pattern_matches(&permission.model_pattern, model)
    })
}
