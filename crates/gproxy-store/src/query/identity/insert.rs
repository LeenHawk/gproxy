use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, insert, unsigned, value};
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
        &[
            "name",
            "organization_id",
            "team_id",
            "password_hash",
            "enabled",
            "is_admin",
        ],
        vec![
            value(input.name.clone()),
            value(input.organization_id),
            value(input.team_id),
            value(input.password_hash.clone()),
            value(input.enabled),
            value(input.is_admin),
        ],
    )
}

pub(crate) fn insert_user_key(input: &UserKeyInput) -> Result<Statement, StoreError> {
    insert(
        "user_keys",
        &[
            "user_id",
            "digest",
            "label",
            "expires_at",
            "enabled",
            "digest_version",
            "prefix",
            "ciphertext",
            "wrapped_key",
            "payload_nonce",
            "key_nonce",
        ],
        vec![
            value(input.user_id),
            value(input.digest.clone()),
            value(input.label.clone()),
            value(input.expires_at),
            value(input.enabled),
            value(i64::from(input.digest_version)),
            value(input.prefix.clone()),
            value(input.envelope.ciphertext.clone()),
            value(input.envelope.wrapped_key.clone()),
            value(input.envelope.payload_nonce.clone()),
            value(input.envelope.key_nonce.clone()),
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
            "quota_total",
            "quota_daily",
            "quota_weekly",
            "quota_monthly",
            "quota_5h",
            "quota_7d",
            "enabled",
        ],
        vec![
            value(input.subject_kind.clone()),
            value(input.subject_id),
            value(decimal(input.quota_total)),
            value(input.quota_daily.map(decimal)),
            value(input.quota_weekly.map(decimal)),
            value(input.quota_monthly.map(decimal)),
            value(input.quota_5h.map(decimal)),
            value(input.quota_7d.map(decimal)),
            value(input.enabled),
        ],
    )
}
