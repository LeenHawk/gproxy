use crate::dto::{
    OrganizationDto, PermissionDto, QuotaDto, RateLimitDto, TeamDto, UserDto, UserKeyDto,
};

pub(super) fn organization(value: &gproxy_store::records::OrganizationRecord) -> OrganizationDto {
    OrganizationDto {
        id: value.id,
        name: value.name.clone(),
        enabled: value.enabled,
    }
}

pub(super) fn team(value: &gproxy_store::records::TeamRecord) -> TeamDto {
    TeamDto {
        id: value.id,
        organization_id: value.organization_id,
        name: value.name.clone(),
        enabled: value.enabled,
    }
}

pub(super) fn user(value: &gproxy_store::records::UserRecord) -> UserDto {
    UserDto {
        id: value.id,
        name: value.name.clone(),
        organization_id: value.organization_id,
        team_id: value.team_id,
        enabled: value.enabled,
    }
}

pub(super) fn user_key(value: &gproxy_store::records::UserKeyRecord) -> UserKeyDto {
    UserKeyDto {
        id: value.id,
        user_id: value.user_id,
        prefix: value.prefix.clone(),
        label: value.label.clone(),
        revealable: value.revealable,
        expires_at: value.expires_at,
        enabled: value.enabled,
    }
}

pub(super) fn permission(value: &gproxy_store::records::PermissionRecord) -> PermissionDto {
    PermissionDto {
        id: value.id,
        subject_kind: value.subject_kind.clone(),
        subject_id: value.subject_id,
        provider_id: value.provider_id,
        operation_group: value.operation_group.clone(),
        allowed: value.allowed,
    }
}

pub(super) fn rate_limit(value: &gproxy_store::records::RateLimitRecord) -> RateLimitDto {
    RateLimitDto {
        id: value.id,
        subject_kind: value.subject_kind.clone(),
        subject_id: value.subject_id,
        requests: value.requests,
        window_seconds: value.window_seconds,
    }
}

pub(super) fn quota(value: &gproxy_store::records::QuotaRecord) -> QuotaDto {
    QuotaDto {
        id: value.id,
        subject_kind: value.subject_kind.clone(),
        subject_id: value.subject_id,
        quota_total: decimal(value.quota_total),
        quota_daily: value.quota_daily.map(decimal),
        quota_weekly: value.quota_weekly.map(decimal),
        quota_monthly: value.quota_monthly.map(decimal),
        quota_5h: value.quota_5h.map(decimal),
        quota_7d: value.quota_7d.map(decimal),
        enabled: value.enabled,
    }
}

fn decimal(value: rust_decimal::Decimal) -> String {
    value.normalize().to_string()
}
