use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, unsigned, update, value};
use crate::records::{
    OrganizationInput, PermissionInput, QuotaInput, RateLimitInput, TeamInput, UserInput,
    UserKeyUpdateInput,
};

pub(crate) fn update_organization(
    id: i64,
    input: &OrganizationInput,
) -> Result<Statement, StoreError> {
    update(
        "organizations",
        id,
        &["name", "enabled"],
        vec![value(input.name.clone()), value(input.enabled)],
    )
}

pub(crate) fn update_team(id: i64, input: &TeamInput) -> Result<Statement, StoreError> {
    update(
        "teams",
        id,
        &["organization_id", "name", "enabled"],
        vec![
            value(input.organization_id),
            value(input.name.clone()),
            value(input.enabled),
        ],
    )
}

pub(crate) fn update_user(id: i64, input: &UserInput) -> Result<Statement, StoreError> {
    update(
        "users",
        id,
        &["name", "organization_id", "team_id", "enabled", "is_admin"],
        vec![
            value(input.name.clone()),
            value(input.organization_id),
            value(input.team_id),
            value(input.enabled),
            value(input.is_admin),
        ],
    )
}

pub(crate) fn update_user_key(
    id: i64,
    input: &UserKeyUpdateInput,
) -> Result<Statement, StoreError> {
    update(
        "user_keys",
        id,
        &["label", "expires_at", "enabled"],
        vec![
            value(input.label.clone()),
            value(input.expires_at),
            value(input.enabled),
        ],
    )
}

pub(crate) fn update_permission(id: i64, input: &PermissionInput) -> Result<Statement, StoreError> {
    update(
        "permissions",
        id,
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

pub(crate) fn update_rate_limit(id: i64, input: &RateLimitInput) -> Result<Statement, StoreError> {
    update(
        "rate_limits",
        id,
        &["subject_kind", "subject_id", "requests", "window_seconds"],
        vec![
            value(input.subject_kind.clone()),
            value(input.subject_id),
            value(unsigned(input.requests, "requests")?),
            value(unsigned(input.window_seconds, "window_seconds")?),
        ],
    )
}

pub(crate) fn update_quota(id: i64, input: &QuotaInput) -> Result<Statement, StoreError> {
    update(
        "quotas",
        id,
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
            value(input.quota_total.map(decimal)),
            value(input.quota_daily.map(decimal)),
            value(input.quota_weekly.map(decimal)),
            value(input.quota_monthly.map(decimal)),
            value(input.quota_5h.map(decimal)),
            value(input.quota_7d.map(decimal)),
            value(input.enabled),
        ],
    )
}
