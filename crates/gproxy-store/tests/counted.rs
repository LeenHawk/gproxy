use gproxy_store::{Store, operations::counted::CountedCharge};
use sea_orm::{ConnectOptions, Database, DbBackend};

#[tokio::test]
async fn counted_windows_charge_atomically_up_to_the_limit() {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    let charge = |amount: i64| CountedCharge {
        credential_id: "c".into(),
        dimension: "hourly".into(),
        window_start_ms: 1_000,
        window_end_ms: 3_601_000,
        amount,
        limit: 5,
    };
    let windows = store.counted_windows();
    let first = windows.charge_many(vec![charge(2)]).await.unwrap()[0];
    assert!(first.applied);
    assert_eq!(first.used, 2);
    let second = windows
        .charge_many(vec![charge(3), charge(1)])
        .await
        .unwrap();
    assert!(second[0].applied && second[0].used == 5);
    assert!(!second[1].applied, "over the limit: refused");
    assert_eq!(second[1].used, 5);
    let refused = windows.charge_many(vec![charge(1)]).await.unwrap()[0];
    assert!(!refused.applied);
    let live = windows.live(2_000).await.unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].used, 5);
    assert!(windows.live(4_000_000).await.unwrap().is_empty());
    let oversized = windows
        .charge_many(vec![CountedCharge {
            dimension: "big".into(),
            amount: 9,
            ..charge(0)
        }])
        .await
        .unwrap()[0];
    assert!(
        !oversized.applied,
        "a first charge over the limit opens the window at zero"
    );
    assert_eq!(oversized.used, 0);
}
