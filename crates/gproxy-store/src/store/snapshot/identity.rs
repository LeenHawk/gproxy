use super::unsigned;
use crate::StoreError;
use crate::backend::QueryResult;
use crate::records::{PermissionRecord, QuotaRecord, RateLimitRecord, UserKeyRecord, UserRecord};

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
                token_limit: unsigned(row.i64("token_limit")?, "token_limit")?,
                window_seconds: unsigned(row.i64("window_seconds")?, "window_seconds")?,
            })
        })
        .collect()
}
