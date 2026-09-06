use gproxy_store::records::{
    OrganizationInput, PermissionInput, QuotaInput, RateLimitInput, TeamInput, UserInput,
};

use crate::AdminError;
use crate::dto::{
    OrganizationWriteRequest, PermissionWriteRequest, QuotaWriteRequest, RateLimitWriteRequest,
    TeamWriteRequest, UserWriteRequest,
};

pub(super) fn user(request: UserWriteRequest) -> Result<UserInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest("user name must not be blank".into()));
    }
    Ok(UserInput {
        name: request.name,
        organization_id: request.organization_id,
        team_id: request.team_id,
        password_hash: request
            .password
            .map(|password| {
                crate::auth::password::validate(&password)?;
                crate::auth::password::hash(&password)
            })
            .transpose()?,
        enabled: request.enabled,
        is_admin: request.is_admin,
    })
}

pub(super) fn organization(
    request: OrganizationWriteRequest,
) -> Result<OrganizationInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "organization name must not be blank".into(),
        ));
    }
    Ok(OrganizationInput {
        name: request.name,
        enabled: request.enabled,
    })
}

pub(super) fn team(request: TeamWriteRequest) -> Result<TeamInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest("team name must not be blank".into()));
    }
    Ok(TeamInput {
        organization_id: request.organization_id,
        name: request.name,
        enabled: request.enabled,
    })
}

pub(super) fn permission(request: PermissionWriteRequest) -> Result<PermissionInput, AdminError> {
    validate_subject(&request.subject_kind)?;
    Ok(PermissionInput {
        subject_kind: request.subject_kind,
        subject_id: request.subject_id,
        provider_id: request.provider_id,
        operation_group: request.operation_group,
        allowed: request.allowed,
    })
}

pub(super) fn rate_limit(request: RateLimitWriteRequest) -> Result<RateLimitInput, AdminError> {
    validate_subject(&request.subject_kind)?;
    if request.requests == 0 || request.window_seconds == 0 {
        return Err(AdminError::BadRequest(
            "rate limit values must be positive".into(),
        ));
    }
    Ok(RateLimitInput {
        subject_kind: request.subject_kind,
        subject_id: request.subject_id,
        requests: request.requests,
        window_seconds: request.window_seconds,
    })
}

pub(super) fn quota(request: QuotaWriteRequest) -> Result<QuotaInput, AdminError> {
    if request.subject_kind != "credential" {
        validate_subject(&request.subject_kind)?;
    }
    let parse = |value: Option<String>, field: &'static str| {
        value.map(|value| decimal(&value, field)).transpose()
    };
    let input = QuotaInput {
        subject_kind: request.subject_kind,
        subject_id: request.subject_id,
        quota_total: parse(request.quota_total, "quota_total")?,
        quota_daily: parse(request.quota_daily, "quota_daily")?,
        quota_weekly: parse(request.quota_weekly, "quota_weekly")?,
        quota_monthly: parse(request.quota_monthly, "quota_monthly")?,
        quota_5h: parse(request.quota_5h, "quota_5h")?,
        quota_7d: parse(request.quota_7d, "quota_7d")?,
        enabled: request.enabled,
    };
    if [
        input.quota_total,
        input.quota_daily,
        input.quota_weekly,
        input.quota_monthly,
        input.quota_5h,
        input.quota_7d,
    ]
    .into_iter()
    .flatten()
    .any(|value| {
        value < rust_decimal::Decimal::ZERO
            || (input.subject_kind != "credential" && value == rust_decimal::Decimal::ZERO)
    }) {
        return Err(AdminError::BadRequest(
            "quota values must be positive".into(),
        ));
    }
    Ok(input)
}

fn decimal(value: &str, field: &'static str) -> Result<rust_decimal::Decimal, AdminError> {
    value
        .parse()
        .map_err(|_| AdminError::BadRequest(format!("{field} must be a decimal")))
}

fn validate_subject(value: &str) -> Result<(), AdminError> {
    match value {
        "user_key" | "user" | "organization" | "team" => Ok(()),
        _ => Err(AdminError::BadRequest("unknown subject kind".into())),
    }
}
