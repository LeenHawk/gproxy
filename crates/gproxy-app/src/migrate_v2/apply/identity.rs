use gproxy_store::records::{PermissionInput, QuotaInput, RecordBatch, UserKeyInput};

use super::{Context, id, mapped, mark, optional};
use crate::migrate_v2::model::SourceData;
use crate::migrate_v2::report::ImportCount;

pub(super) async fn base(
    context: &mut Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    context.organizations = mapped(
        context,
        &data.organizations,
        RecordBatch::Organizations(
            data.organizations
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        ),
    )
    .await?;
    mark(counts, "organizations", data.organizations.len());

    let teams = data
        .teams
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.organization_id = id(&context.organizations, input.organization_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.teams = mapped(context, &data.teams, RecordBatch::Teams(teams)).await?;
    mark(counts, "teams", data.teams.len());

    let users = data
        .users
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.organization_id = optional(&context.organizations, input.organization_id)?;
            input.team_id = optional(&context.teams, input.team_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.users = mapped(context, &data.users, RecordBatch::Users(users)).await?;
    for value in data.users.iter().filter(|value| value.value.is_admin) {
        context
            .store
            .insert_permission(&PermissionInput {
                subject_kind: "user".into(),
                subject_id: id(&context.users, value.id)?,
                provider_id: None,
                operation_group: None,
                allowed: true,
            })
            .await?;
    }
    mark(counts, "users", data.users.len());
    for value in &data.permissions {
        if value.value.scope == "user"
            && data
                .users
                .iter()
                .any(|user| user.id == value.value.scope_id && user.value.is_admin)
        {
            continue;
        }
        let (kind, map) = match value.value.scope.as_str() {
            "org" => ("organization", &context.organizations),
            "team" => ("team", &context.teams),
            "user" => ("user", &context.users),
            _ => {
                return Err(crate::AppError::Migration(
                    "invalid permission scope after validation".into(),
                ));
            }
        };
        context
            .store
            .insert_permission(&PermissionInput {
                subject_kind: kind.into(),
                subject_id: id(map, value.value.scope_id)?,
                provider_id: None,
                operation_group: None,
                allowed: true,
            })
            .await?;
    }
    mark(counts, "route_permissions", data.permissions.len());
    Ok(())
}

pub(super) async fn keys_and_quotas(
    context: &mut Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let keys = data
        .user_keys
        .iter()
        .map(|value| {
            let api_key = &value.value.stored_key;
            let digest =
                crate::control::user_key_digest(crate::control::USER_KEY_DIGEST_VERSION, api_key)
                    .expect("current digest version is supported");
            Ok(UserKeyInput {
                user_id: id(&context.users, value.value.user_id)?,
                digest,
                digest_version: crate::control::USER_KEY_DIGEST_VERSION,
                prefix: api_key.chars().take(12).collect(),
                envelope: context
                    .cipher
                    .seal_user_key(&serde_json::Value::String(api_key.clone()))?,
                label: value.value.label.clone(),
                expires_at: None,
                enabled: value.value.enabled,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.user_keys = mapped(context, &data.user_keys, RecordBatch::UserKeys(keys)).await?;
    mark(counts, "user_keys", data.user_keys.len());

    let quotas = data
        .quotas
        .iter()
        .map(|value| {
            let (kind, map) = match value.value.scope.as_str() {
                "org" => ("organization", &context.organizations),
                "team" => ("team", &context.teams),
                "user" => ("user", &context.users),
                _ => {
                    return Err(crate::AppError::Migration(
                        "invalid quota scope after validation".into(),
                    ));
                }
            };
            Ok(QuotaInput {
                subject_kind: kind.into(),
                subject_id: id(map, value.value.scope_id)?,
                quota_total: value.value.quota_total,
                quota_daily: value.value.quota_daily,
                quota_weekly: value.value.quota_weekly,
                quota_monthly: value.value.quota_monthly,
                quota_5h: value.value.quota_5h,
                quota_7d: value.value.quota_7d,
                enabled: true,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::Quotas(quotas))
        .await?;
    mark(counts, "quotas", data.quotas.len());
    Ok(())
}
