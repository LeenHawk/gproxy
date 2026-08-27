use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::select_all;

pub(crate) fn select_organizations() -> Result<Statement, StoreError> {
    select_all("organizations", &["id", "name", "enabled"])
}

pub(crate) fn select_teams() -> Result<Statement, StoreError> {
    select_all("teams", &["id", "organization_id", "name", "enabled"])
}

pub(crate) fn select_users() -> Result<Statement, StoreError> {
    select_all(
        "users",
        &[
            "id",
            "name",
            "organization_id",
            "team_id",
            "enabled",
            "is_admin",
        ],
    )
}

pub(crate) fn select_user_keys() -> Result<Statement, StoreError> {
    select_all(
        "user_keys",
        &[
            "id",
            "user_id",
            "digest",
            "digest_version",
            "prefix",
            "label",
            "ciphertext",
            "wrapped_key",
            "payload_nonce",
            "key_nonce",
            "expires_at",
            "enabled",
        ],
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
            "quota_total",
            "quota_daily",
            "quota_weekly",
            "quota_monthly",
            "quota_5h",
            "quota_7d",
            "enabled",
        ],
    )
}
