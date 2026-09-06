use super::super::{Executor, Statement};
use super::{libsql_store, native_store};
use crate::schema::{Dialect, SchemaVersion};

#[tokio::test]
async fn credential_budget_migration_preserves_costs_and_makes_total_optional() {
    for remote in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("budget.db");
        let old = super::super::native::NativeSql::open(path.clone())
            .await
            .unwrap();
        crate::migration::migrate_to(&old, Dialect::NativeSqlite, SchemaVersion::OAuthSessions)
            .await
            .unwrap();
        old.batch(vec![
            Statement::plain("INSERT INTO quotas(id,subject_kind,subject_id,quota_total,enabled) VALUES(7,'user',1,'100',1)"),
            Statement::plain("INSERT INTO quota_windows(id,quota_id,window_kind,window_start,cost_used,active_slot) VALUES(8,7,'total',0,'12.34',1)"),
            Statement::plain("INSERT INTO quota_settlements(request_id,window_id,cost) VALUES('before-upgrade',8,'12.34')"),
        ]).await.unwrap();
        drop(old);
        let (store, _) = if remote {
            libsql_store(path).await.unwrap()
        } else {
            native_store(path).await.unwrap()
        };
        let quota = store
            .control_snapshot()
            .await
            .unwrap()
            .quotas
            .into_iter()
            .find(|quota| quota.id == 7)
            .unwrap();
        assert_eq!(quota.quota_total, Some(100.into()));
        let window = store
            .add_quota_cost("before-upgrade", 8, "12.34".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(window.cost_used.to_string(), "12.34");
        let id = store
            .insert_quota(&crate::records::QuotaInput {
                subject_kind: "credential".into(),
                subject_id: 1,
                quota_total: None,
                quota_daily: Some(1.into()),
                quota_weekly: None,
                quota_monthly: None,
                quota_5h: None,
                quota_7d: None,
                enabled: true,
            })
            .await
            .unwrap();
        assert!(
            store
                .control_snapshot()
                .await
                .unwrap()
                .quotas
                .iter()
                .any(|quota| quota.id == id && quota.quota_total.is_none())
        );
    }
}
