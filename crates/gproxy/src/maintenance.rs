//! Retain configuration and settled balances; prune completed request history
//! and, on its own clock, the upstream quota observation log; drop rows whose
//! expiry has passed.
use crate::Result;
use gproxy_store::{
    Store,
    entity::{
        identity::user_session,
        limits::{credential_block, credential_quota_cycle},
        oauth::{code, device, token},
        resource::protocol_state,
        usage::{capture_record, usage_record},
    },
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

const BATCH: u64 = 256;

/// How long an OAuth code, token or device row outlives its expiry. Presenting
/// a code or refresh token that was already consumed is how a leaked one is
/// detected, and it revokes the whole grant; that only works while the row is
/// there to say it was consumed. Once deleted, the same replay is merely an
/// unknown credential. Expired rows are refused either way, so the week is
/// spent only on noticing a replay, and bounds what a table of spent codes and
/// rotated tokens can grow to.
const OAUTH_REPLAY_WINDOW_MS: i64 = 7 * 86_400_000;

pub async fn clean(
    db: &DatabaseConnection,
    days: Option<u32>,
    observation_days: Option<u32>,
    max_mb: Option<i64>,
    now: i64,
) -> Result<u64> {
    let mut expired = 0;
    loop {
        let count = prune_expired(db, now).await?;
        expired += count;
        if count == 0 {
            break;
        }
        tokio::task::yield_now().await;
    }
    if expired > 0 {
        tracing::info!(
            removed = expired,
            "expired blocks, sessions and oauth rows removed"
        );
    }
    let mut removed = 0;
    let mut reclaim = false;
    if let Some(days) = observation_days {
        let cutoff = now.saturating_sub(i64::from(days) * 86_400_000);
        loop {
            let count = prune_observations(db, cutoff).await?;
            removed += count;
            if count == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    }
    if let Some(days) = days {
        let cutoff = now.saturating_sub(i64::from(days) * 86_400_000);
        loop {
            let count = prune(db, cutoff).await?;
            removed += count;
            if count == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    }
    if let Some(max_mb) = max_mb.filter(|value| *value > 0)
        && db.get_database_backend() == DbBackend::Sqlite
    {
        let limit = max_mb.saturating_mul(1024 * 1024);
        reclaim = physical_bytes(db).await? > limit;
        while occupied_bytes(db).await? > limit {
            let count = prune(db, now).await?;
            removed += count;
            if count == 0 {
                tracing::warn!(
                    max_mb,
                    "database exceeds its history budget; remaining data is configuration or active work"
                );
                break;
            }
            tokio::task::yield_now().await;
        }
    }
    // Rows pruned by age leave free pages SQLite reuses for the next rows, so
    // the file needs no rewrite for them. VACUUM rewrites the whole file on
    // the one connection every request needs, so it runs only to bring a file
    // over its size budget back under it, and only once a tenth of it is free:
    // right after a VACUUM nothing is, so it cannot run again every minute.
    if reclaim && db.get_database_backend() == DbBackend::Sqlite && free_fraction(db).await? >= 0.1
    {
        db.execute_unprepared("VACUUM").await?;
        db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
            .await?;
    }
    if removed > 0 {
        tracing::info!(removed, "request history cleanup completed");
    }
    Ok(removed + expired)
}

async fn physical_bytes(db: &DatabaseConnection) -> Result<i64> {
    let row = db
        .query_one_raw(sea_orm::Statement::from_string(
            DbBackend::Sqlite,
            "SELECT page_count * page_size AS bytes FROM pragma_page_count(), pragma_page_size()",
        ))
        .await?
        .unwrap();
    Ok(row.try_get("", "bytes")?)
}

/// The share of the file's pages that are free.
async fn free_fraction(db: &DatabaseConnection) -> Result<f64> {
    let (pages, free, _) = page_counts(db).await?;
    Ok(if pages == 0 {
        0.0
    } else {
        free as f64 / pages as f64
    })
}

async fn occupied_bytes(db: &DatabaseConnection) -> Result<i64> {
    let (pages, free, size) = page_counts(db).await?;
    Ok((pages - free) * size)
}

/// `page_count`, `freelist_count` and `page_size`.
async fn page_counts(db: &DatabaseConnection) -> Result<(i64, i64, i64)> {
    let mut values = Vec::new();
    for sql in [
        "PRAGMA page_count",
        "PRAGMA freelist_count",
        "PRAGMA page_size",
    ] {
        let row = db
            .query_one_raw(sea_orm::Statement::from_string(DbBackend::Sqlite, sql))
            .await?
            .unwrap();
        values.push(row.try_get_by_index::<i64>(0)?);
    }
    Ok((values[0], values[1], values[2]))
}

async fn prune(db: &DatabaseConnection, cutoff: i64) -> Result<u64> {
    let store = Store::new(db.clone());
    let captures: Vec<String> = capture_record::Entity::find()
        .select_only()
        .column(capture_record::Column::Id)
        .filter(capture_record::Column::EndedAtMs.lt(cutoff))
        .filter(capture_record::Column::State.ne(capture_record::CaptureState::InProgress))
        .order_by_asc(capture_record::Column::EndedAtMs)
        .limit(BATCH)
        .into_tuple()
        .all(db)
        .await?;
    let usage: Vec<String> = usage_record::Entity::find()
        .select_only()
        .column(usage_record::Column::RequestId)
        .filter(usage_record::Column::EndedAtMs.lt(cutoff))
        .order_by_asc(usage_record::Column::EndedAtMs)
        .limit(BATCH)
        .into_tuple()
        .all(db)
        .await?;
    // Capture events and links have cascading foreign keys. Usage is history,
    // not the settled quota counters, billing entries or subscription windows.
    store.capture_records().delete_many(&captures).await?;
    store.usage_records().delete_many(&usage).await?;
    Ok((captures.len() + usage.len()) as u64)
}

/// Quota observations older than `cutoff`, oldest first. Only the raw log:
/// the cycles they were folded into (`credential_cycles`) are kept, and so
/// are observations a live block still names, which cannot be this old.
async fn prune_observations(db: &DatabaseConnection, cutoff: i64) -> Result<u64> {
    let ids: Vec<String> = credential_quota_cycle::Entity::find()
        .select_only()
        .column(credential_quota_cycle::Column::Id)
        .filter(credential_quota_cycle::Column::ObservedAtMs.lt(cutoff))
        .order_by_asc(credential_quota_cycle::Column::ObservedAtMs)
        .limit(BATCH)
        .into_tuple()
        .all(db)
        .await?;
    Store::new(db.clone())
        .credential_quota_cycles()
        .delete_many(&ids)
        .await?;
    Ok(ids.len() as u64)
}

/// One batch of each kind of row that is past its expiry. A credential block
/// no longer blocks anything once `until_ms` has passed and a session no
/// longer signs anyone in; OAuth rows wait out `OAUTH_REPLAY_WINDOW_MS` first.
async fn prune_expired(db: &DatabaseConnection, now: i64) -> Result<u64> {
    let store = Store::new(db.clone());
    let spent = now.saturating_sub(OAUTH_REPLAY_WINDOW_MS);
    let blocks = ids_before::<credential_block::Entity>(
        db,
        credential_block::Column::Id,
        credential_block::Column::UntilMs,
        now,
    )
    .await?;
    store.credential_blocks().delete_many(&blocks).await?;
    let sessions = ids_before::<user_session::Entity>(
        db,
        user_session::Column::Id,
        user_session::Column::ExpiresAtMs,
        now,
    )
    .await?;
    store.user_sessions().delete_many(&sessions).await?;
    let codes =
        ids_before::<code::Entity>(db, code::Column::Id, code::Column::ExpiresAtMs, spent).await?;
    store.oauth_codes().delete_many(&codes).await?;
    let tokens =
        ids_before::<token::Entity>(db, token::Column::Id, token::Column::ExpiresAtMs, spent)
            .await?;
    store.oauth_tokens().delete_many(&tokens).await?;
    let devices =
        ids_before::<device::Entity>(db, device::Column::Id, device::Column::ExpiresAtMs, spent)
            .await?;
    store.oauth_devices().delete_many(&devices).await?;
    // Protocol state (Responses continuation history, video jobs) is read as
    // absent once expired, but nothing else would ever remove the row: a key
    // is only written again by the conversation or job that owns it. A row
    // with no expiry never expires.
    let states: Vec<(String, String)> = protocol_state::Entity::find()
        .select_only()
        .column(protocol_state::Column::Scope)
        .column(protocol_state::Column::Key)
        .filter(protocol_state::Column::ExpiresAtMs.lt(now))
        .order_by_asc(protocol_state::Column::ExpiresAtMs)
        .limit(BATCH)
        .into_tuple()
        .all(db)
        .await?;
    store.protocol_states().delete_many(&states).await?;
    Ok(
        (blocks.len() + sessions.len() + codes.len() + tokens.len() + devices.len() + states.len())
            as u64,
    )
}

/// Up to `BATCH` ids of rows whose `at` is before `cutoff`, oldest first.
async fn ids_before<E: EntityTrait>(
    db: &DatabaseConnection,
    id: E::Column,
    at: E::Column,
    cutoff: i64,
) -> Result<Vec<String>> {
    Ok(E::find()
        .select_only()
        .column(id)
        .filter(at.lt(cutoff))
        .order_by_asc(at)
        .limit(BATCH)
        .into_tuple()
        .all(db)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ActiveModelTrait, Set};
    async fn fixture() -> DatabaseConnection {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        Store::new(db.clone()).sync().await.unwrap();
        Store::new(db.clone())
            .settings()
            .update(Default::default())
            .await
            .unwrap();
        for (id, ended, bytes) in [
            ("old", Some(1), 2_000_000),
            ("active", None, 100),
            ("recent", Some(200_000_000), 100),
        ] {
            capture_record::ActiveModel {
                id: Set(id.into()),
                side: Set(capture_record::CaptureSide::Downstream),
                kind: Set(capture_record::CaptureKind::Http),
                started_at_ms: Set(0),
                ended_at_ms: Set(ended),
                state: Set(if ended.is_some() {
                    capture_record::CaptureState::Completed
                } else {
                    capture_record::CaptureState::InProgress
                }),
                response_body: Set(Some(vec![1; bytes])),
                ..Default::default()
            }
            .insert(&db)
            .await
            .unwrap();
        }
        db
    }
    #[tokio::test]
    async fn retention_keeps_active_work_and_configuration() {
        let db = fixture().await;
        assert_eq!(
            clean(&db, Some(1), None, None, 200_000_000).await.unwrap(),
            1
        );
        assert!(
            capture_record::Entity::find_by_id("active")
                .one(&db)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            capture_record::Entity::find_by_id("recent")
                .one(&db)
                .await
                .unwrap()
                .is_some()
        );
        assert!(Store::new(db).settings().get().await.unwrap().is_some());
    }
    #[tokio::test]
    async fn expired_sessions_go_now_and_spent_oauth_tokens_after_the_replay_window() {
        use gproxy_store::entity::{
            identity::{api_key, user},
            oauth::{client, grant},
        };
        let db = fixture().await;
        let day = 86_400_000;
        let now = 100 * day;
        user::ActiveModel {
            id: Set("u".into()),
            name: Set("u".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        for (id, expires) in [("gone", now - 1), ("live", now + day)] {
            user_session::ActiveModel {
                id: Set(id.into()),
                user_id: Set("u".into()),
                token_hash: Set(id.into()),
                created_at_ms: Set(0),
                expires_at_ms: Set(expires),
            }
            .insert(&db)
            .await
            .unwrap();
        }
        api_key::ActiveModel {
            id: Set("k".into()),
            user_id: Set("u".into()),
            name: Set("k".into()),
            key_hash: Set("k".into()),
            prefix: Set("k".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        client::ActiveModel {
            id: Set("c".into()),
            name: Set("c".into()),
            redirect_uris: Set(serde_json::json!([])),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        grant::ActiveModel {
            id: Set("g".into()),
            user_id: Set("u".into()),
            api_key_id: Set("k".into()),
            client_id: Set("c".into()),
            scopes: Set(serde_json::json!([])),
            subject: Set("u".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        for (id, expires) in [("spent", now - 8 * day), ("replayable", now - day)] {
            token::ActiveModel {
                id: Set(id.into()),
                token_hash: Set(id.as_bytes().to_vec()),
                grant_id: Set("g".into()),
                kind: Set(token::TokenKind::Refresh),
                created_at_ms: Set(0),
                expires_at_ms: Set(expires),
                consumed_at_ms: Set(Some(0)),
                ..Default::default()
            }
            .insert(&db)
            .await
            .unwrap();
        }
        // Retention is off: expiry is not history and is pruned regardless.
        assert_eq!(clean(&db, None, None, None, now).await.unwrap(), 2);
        let sessions: Vec<String> = user_session::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(sessions, ["live"]);
        let tokens: Vec<String> = token::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(
            tokens,
            ["replayable"],
            "a consumed token is kept a week past expiry, so its replay still revokes"
        );
    }

    #[tokio::test]
    async fn sqlite_budget_reclaims_completed_history() {
        let db = fixture().await;
        assert!(occupied_bytes(&db).await.unwrap() > 1024 * 1024);
        assert!(clean(&db, None, None, Some(1), 300_000_000).await.unwrap() > 0);
        assert!(occupied_bytes(&db).await.unwrap() <= 1024 * 1024);
        assert!(
            capture_record::Entity::find_by_id("active")
                .one(&db)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn observation_retention_prunes_the_log_and_keeps_the_cycles() {
        use gproxy_store::entity::limits::credential_cycle::{self, CycleBoundary, CycleOpening};
        let db = fixture().await;
        let store = Store::new(db.clone());
        let day = 86_400_000;
        let now = 100 * day;
        store
            .credential_quota_cycles()
            .create_many(
                [("old", now - 91 * day), ("recent", now - 89 * day)]
                    .into_iter()
                    .map(|(id, at)| credential_quota_cycle::ActiveModel {
                        id: Set(id.into()),
                        credential_id: Set("c".into()),
                        scope: Set(serde_json::json!("all")),
                        snapshot: Set(serde_json::json!({})),
                        observed_at_ms: Set(at),
                        credential_cycle_id: Set(Some("cycle".into())),
                        ..Default::default()
                    })
                    .collect(),
            )
            .await
            .unwrap();
        store
            .credential_cycles()
            .create_many(vec![credential_cycle::ActiveModel {
                id: Set("cycle".into()),
                credential_id: Set("c".into()),
                closed_at_ms: Set(Some(now - 91 * day)),
                window_id: Set("five_hour".into()),
                scope: Set(serde_json::json!("all")),
                starts_at_ms: Set(now - 92 * day),
                boundary: Set(CycleBoundary::Observed),
                opened_by: Set(CycleOpening::FirstUse),
                cost_usd: Set(gproxy_store::FixedDecimal::ZERO),
                ..Default::default()
            }])
            .await
            .unwrap();
        // Request history is off; the observation clock runs on its own.
        assert_eq!(clean(&db, None, Some(90), None, now).await.unwrap(), 1);
        let left: Vec<String> = credential_quota_cycle::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(left, ["recent"]);
        assert!(
            credential_cycle::Entity::find_by_id("cycle".to_owned())
                .one(&db)
                .await
                .unwrap()
                .is_some()
        );
        // None keeps every observation.
        assert_eq!(clean(&db, None, None, None, now * 2).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn expired_protocol_state_is_pruned_and_unexpiring_state_kept() {
        let db = fixture().await;
        let now = 1_000_000;
        for (key, expires) in [
            ("gone", Some(now - 1)),
            ("live", Some(now + 1)),
            ("forever", None),
        ] {
            protocol_state::ActiveModel {
                scope: Set("s".into()),
                key: Set(key.into()),
                version: Set(vec![1]),
                payload: Set(vec![]),
                expires_at_ms: Set(expires),
            }
            .insert(&db)
            .await
            .unwrap();
        }
        assert_eq!(clean(&db, None, None, None, now).await.unwrap(), 1);
        let mut left: Vec<String> = protocol_state::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.key)
            .collect();
        left.sort();
        assert_eq!(left, ["forever", "live"]);
    }
}
