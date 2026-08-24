use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::select_all;

pub(crate) fn select_users() -> Result<Statement, StoreError> {
    select_all(
        "users",
        &["id", "name", "organization_id", "team_id", "enabled"],
    )
}

pub(crate) fn select_user_keys() -> Result<Statement, StoreError> {
    select_all(
        "user_keys",
        &["id", "user_id", "digest", "expires_at", "enabled"],
    )
}

pub(crate) fn select_permissions() -> Result<Statement, StoreError> {
    select_all(
        "permissions",
        &[
            "id",
            "subject_kind",
            "subject_id",
            "provider_id",
            "operation_group",
            "allowed",
        ],
    )
}

pub(crate) fn select_rate_limits() -> Result<Statement, StoreError> {
    select_all(
        "rate_limits",
        &[
            "id",
            "subject_kind",
            "subject_id",
            "requests",
            "window_seconds",
        ],
    )
}

pub(crate) fn select_quotas() -> Result<Statement, StoreError> {
    select_all(
        "quotas",
        &[
            "id",
            "subject_kind",
            "subject_id",
            "token_limit",
            "window_seconds",
        ],
    )
}
