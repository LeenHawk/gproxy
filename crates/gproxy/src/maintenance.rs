//! Retain configuration and settled balances; prune completed request history
//! and, on its own clock, the upstream quota observation log.
use crate::Result;
use gproxy_store::{
    Store,
    entity::{
        limits::credential_quota_cycle,
        usage::{capture_record, usage_record},
    },
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect,
};

const BATCH: u64 = 256;

pub async fn clean(
    db: &DatabaseConnection,
    days: Option<u32>,
    observation_days: Option<u32>,
    max_mb: Option<i64>,
    now: i64,
) -> Result<u64> {
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
    if removed > 0 || reclaim {
        if db.get_database_backend() == DbBackend::Sqlite {
            // Reclaim freed pages so the on-disk size reflects the cleanup.
            db.execute_unprepared("VACUUM").await?;
            db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE)")
                .await?;
        }
        tracing::info!(removed, "request history cleanup completed");
    }
    Ok(removed)
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

async fn occupied_bytes(db: &DatabaseConnection) -> Result<i64> {
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
    Ok((values[0] - values[1]) * values[2])
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
}
