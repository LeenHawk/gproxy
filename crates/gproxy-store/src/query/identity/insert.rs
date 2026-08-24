use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, unsigned, value};
use crate::records::{
    OrganizationInput, PermissionInput, QuotaInput, RateLimitInput, TeamInput, UserInput,
    UserKeyInput,
};

pub(crate) fn insert_organization(input: &OrganizationInput) -> Result<Statement, StoreError> {
    insert(
        "organizations",
        &["name", "enabled"],
        vec![value(input.name.clone()), value(input.enabled)],
    )
}

pub(crate) fn insert_team(input: &TeamInput) -> Result<Statement, StoreError> {
    insert(
        "teams",
        &["organization_id", "name", "enabled"],
        vec![
            value(input.organization_id),
            value(input.name.clone()),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_user(input: &UserInput) -> Result<Statement, StoreError> {
    insert(
        "users",
        &["name", "organization_id", "team_id", "enabled"],
        vec![
            value(input.name.clone()),
            value(input.organization_id),
            value(input.team_id),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_user_key(input: &UserKeyInput) -> Result<Statement, StoreError> {
    insert(
        "user_keys",
        &["user_id", "digest", "label", "expires_at", "enabled"],
        vec![
            value(input.user_id),
            value(input.digest.clone()),
            value(input.label.clone()),
            value(input.expires_at),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_permission(input: &PermissionInput) -> Result<Statement, StoreError> {
    insert(
        "permissions",
        &[
            "subject_kind",
            "subject_id",
            "provider_id",
            "operation_group",
            "allowed",
        ],
        vec![
            value(input.subject_kind.clone()),
            value(input.subject_id),
            value(input.provider_id),
            value(input.operation_group.clone()),
            value(input.allowed),
        ],
    )
}

pub(crate) fn insert_rate_limit(input: &RateLimitInput) -> Result<Statement, StoreError> {
    insert(
        "rate_limits",
        &["subject_kind", "subject_id", "requests", "window_seconds"],
        vec![
            value(input.subject_kind.clone()),
            value(input.subject_id),
            value(unsigned(input.requests, "requests")?),
            value(unsigned(input.window_seconds, "window_seconds")?),
        ],
    )
}

pub(crate) fn insert_quota(input: &QuotaInput) -> Result<Statement, StoreError> {
    insert(
        "quotas",
        &[
            "subject_kind",
            "subject_id",
            "token_limit",
            "window_seconds",
        ],
        vec![
            value(input.subject_kind.clone()),
            value(input.subject_id),
            value(unsigned(input.token_limit, "token_limit")?),
            value(unsigned(input.window_seconds, "window_seconds")?),
        ],
    )
}
