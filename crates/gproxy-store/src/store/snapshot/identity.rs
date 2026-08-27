use super::{decimal, unsigned, unsigned32};
use crate::StoreError;
use crate::backend::QueryResult;
use crate::records::{
    OrganizationRecord, PermissionRecord, QuotaRecord, RateLimitRecord, TeamRecord, UserKeyRecord,
    UserRecord,
};

pub(super) fn organizations(result: QueryResult) -> Result<Vec<OrganizationRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(OrganizationRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn teams(result: QueryResult) -> Result<Vec<TeamRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(TeamRecord {
                id: row.i64("id")?,
                organization_id: row.i64("organization_id")?,
                name: row.text("name")?.to_owned(),
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn users(result: QueryResult) -> Result<Vec<UserRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(UserRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                organization_id: row.optional_i64("organization_id")?,
                team_id: row.optional_i64("team_id")?,
                enabled: row.i64("enabled")? != 0,
                is_admin: row.i64("is_admin")? != 0,
            })
        })
        .collect()
}

pub(super) fn user_keys(result: QueryResult) -> Result<Vec<UserKeyRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(UserKeyRecord {
                id: row.i64("id")?,
                user_id: row.i64("user_id")?,
                digest: row.blob("digest")?.to_vec(),
                digest_version: unsigned32(row.i64("digest_version")?, "digest_version")?,
                prefix: row.optional_text("prefix")?.map(str::to_owned),
                label: row.optional_text("label")?.map(str::to_owned),
                revealable: ["ciphertext", "wrapped_key", "payload_nonce", "key_nonce"]
                    .into_iter()
                    .map(|column| row.optional_blob(column))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .all(|value| value.is_some()),
                expires_at: row.optional_i64("expires_at")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn permissions(result: QueryResult) -> Result<Vec<PermissionRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(PermissionRecord {
                id: row.i64("id")?,
                subject_kind: row.text("subject_kind")?.to_owned(),
                subject_id: row.i64("subject_id")?,
                provider_id: row.optional_i64("provider_id")?,
                operation_group: row.optional_text("operation_group")?.map(str::to_owned),
                allowed: row.i64("allowed")? != 0,
            })
        })
        .collect()
}

pub(super) fn rate_limits(result: QueryResult) -> Result<Vec<RateLimitRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RateLimitRecord {
                id: row.i64("id")?,
                subject_kind: row.text("subject_kind")?.to_owned(),
                subject_id: row.i64("subject_id")?,
                requests: unsigned(row.i64("requests")?, "requests")?,
                window_seconds: unsigned(row.i64("window_seconds")?, "window_seconds")?,
            })
        })
        .collect()
}

pub(super) fn quotas(result: QueryResult) -> Result<Vec<QuotaRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(QuotaRecord {
                id: row.i64("id")?,
                subject_kind: row.text("subject_kind")?.to_owned(),
                subject_id: row.i64("subject_id")?,
                quota_total: decimal(row.text("quota_total")?, "quota_total")?,
                quota_daily: optional_decimal(&row, "quota_daily")?,
                quota_weekly: optional_decimal(&row, "quota_weekly")?,
                quota_monthly: optional_decimal(&row, "quota_monthly")?,
                quota_5h: optional_decimal(&row, "quota_5h")?,
                quota_7d: optional_decimal(&row, "quota_7d")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

fn optional_decimal(
    row: &crate::backend::Row,
    field: &'static str,
) -> Result<Option<rust_decimal::Decimal>, StoreError> {
    row.optional_text(field)?
        .map(|value| decimal(value, field))
        .transpose()
}
