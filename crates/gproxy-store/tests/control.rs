#![cfg(not(target_arch = "wasm32"))]
//! The control/routing/identity split and the single-batch `load_all_data`.

use gproxy_store::{
    Store,
    entity::{
        config::setting,
        identity::{api_key, user},
        routing::{route, route_member},
        upstream::provider,
    },
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;

async fn seeded() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(7),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("p".into()),
            name: Set("p".into()),
            channel: Set("test".into()),
            config: Set(json!({})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .routes()
        .create_many(vec![route::ActiveModel {
            id: Set("r".into()),
            name: Set("fast".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .route_members()
        .create_many(vec![route_member::ActiveModel {
            id: Set("rm".into()),
            route_id: Set("r".into()),
            provider_id: Set("p".into()),
            upstream_model: Set("m1".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("u".into()),
            name: Set("u".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("k".into()),
            user_id: Set("u".into()),
            name: Set("k".into()),
            key_hash: Set("hash".into()),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
}

#[tokio::test]
async fn load_all_data_returns_the_three_sets_from_one_read() {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    assert_eq!(all.control.settings.unwrap().config_revision, 7);
    assert_eq!(all.control.providers.len(), 1);
    assert_eq!(all.routing.routes[0].id, "r");
    assert_eq!(all.routing.route_members[0].upstream_model, "m1");
    assert_eq!(all.routing.routes[0].name, "fast");
    assert_eq!(all.identity.users[0].id, "u");
    assert_eq!(all.identity.api_keys[0].key_hash, "hash");
    assert!(all.identity.organizations.is_empty());
}

#[tokio::test]
async fn the_three_loaders_agree_with_load_all_data() {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    let control = store.load_control_data().await.unwrap();
    let routing = store.load_routing_data().await.unwrap();
    let identity = store.load_identity_data().await.unwrap();
    assert_eq!(control.providers, all.control.providers);
    assert_eq!(control.settings, all.control.settings);
    assert_eq!(routing.routes, all.routing.routes);
    assert_eq!(identity.users, all.identity.users);
    assert_eq!(identity.api_keys, all.identity.api_keys);
}

/// The core subset is what `Core::load_data` assembles from: routing and
/// identity rows must not travel with it any more.
#[tokio::test]
async fn control_data_carries_neither_routing_nor_identity_rows() {
    let store = seeded().await;
    let control = store.load_control_data().await.unwrap();
    let fields = format!("{control:?}");
    for absent in ["users", "api_keys", "routes", "exposed_models"] {
        assert!(
            !fields.contains(absent),
            "ControlData still carries `{absent}`"
        );
    }
    // What it does carry is the execution subset, unchanged.
    assert_eq!(control.providers.len(), 1);
    assert!(control.credentials.is_empty());
    assert!(control.credential_blocks.is_empty());
}
