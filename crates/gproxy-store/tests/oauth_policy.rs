#![cfg(not(target_arch = "wasm32"))]

use gproxy_store::{
    Store,
    entity::{
        config::setting,
        identity::{api_key, organization, organization_member, team, team_member, user},
        oauth::{client, code, device, grant},
    },
    operations::{
        CasOutcome,
        oauth::{
            Authorization, ClientAccess, DeviceApproval, ExchangeSource, IssuedToken, TokenExchange,
        },
    },
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;

async fn database() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("u".into()),
            name: Set("user".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_clients()
        .create_many(
            ["a", "b", "c"]
                .map(|id| client::ActiveModel {
                    id: Set(id.into()),
                    name: Set(id.into()),
                    redirect_uris: Set(json!([])),
                    ..Default::default()
                })
                .into(),
        )
        .await
        .unwrap();
    store
}
async fn allowed(store: &Store<DatabaseConnection>) -> Vec<bool> {
    store
        .oauth_clients()
        .allowed_many(&["a", "b", "c", "unknown"].map(|id| ClientAccess {
            user_id: "u".into(),
            client_id: id.into(),
        }))
        .await
        .unwrap()
}

#[tokio::test]
async fn same_level_union_cross_level_intersection_and_team_parent_inheritance() {
    let store = database().await;
    assert_eq!(allowed(&store).await, [true, true, true, false]);
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(Some(json!(["a", "b"]))),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .organizations()
        .create_many(
            [
                ("o1", Some(json!(["a", "c"]))),
                ("o2", None),
                ("unrelated", Some(json!(["b"]))),
            ]
            .map(|(id, list)| organization::ActiveModel {
                id: Set(id.into()),
                name: Set(id.into()),
                oauth_client_allowlist: Set(list),
                created_at_ms: Set(0),
            })
            .into(),
        )
        .await
        .unwrap();
    store
        .organization_members()
        .create_many(
            ["o1", "o2"]
                .map(|id| organization_member::ActiveModel {
                    organization_id: Set(id.into()),
                    user_id: Set("u".into()),
                    ..Default::default()
                })
                .into(),
        )
        .await
        .unwrap();
    // An unconfigured peer and an unrelated organization cannot widen o1.
    assert_eq!(allowed(&store).await, [true, false, false, false]);
    store
        .organizations()
        .update_many(vec![organization::ActiveModel {
            id: Set("o2".into()),
            oauth_client_allowlist: Set(Some(json!(["b"]))),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [true, true, false, false]);
    store
        .teams()
        .create_many(
            [("t1", "o1", Some(json!(["c"]))), ("t2", "o2", None)]
                .map(|(id, org, list)| team::ActiveModel {
                    id: Set(id.into()),
                    organization_id: Set(org.into()),
                    name: Set(id.into()),
                    oauth_client_allowlist: Set(list),
                    created_at_ms: Set(0),
                })
                .into(),
        )
        .await
        .unwrap();
    store
        .team_members()
        .create_many(
            ["t1", "t2"]
                .map(|id| team_member::ActiveModel {
                    team_id: Set(id.into()),
                    user_id: Set("u".into()),
                    ..Default::default()
                })
                .into(),
        )
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, false, false, false]);
    store
        .teams()
        .update_many(vec![team::ActiveModel {
            id: Set("t2".into()),
            oauth_client_allowlist: Set(Some(json!(["b"]))),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, true, false, false]);
    store
        .users()
        .update_many(vec![user::ActiveModel {
            id: Set("u".into()),
            oauth_client_allowlist: Set(Some(json!(["a"]))),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, false, false, false]);
    store
        .users()
        .update_many(vec![user::ActiveModel {
            id: Set("u".into()),
            oauth_client_allowlist: Set(None),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(Some(json!([]))),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, false, false, false]);
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(None),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, true, true, false]);
    // Team parents continue to constrain users without direct org memberships.
    store
        .organization_members()
        .delete_many(&[("o1".into(), "u".into()), ("o2".into(), "u".into())])
        .await
        .unwrap();
    store
        .organizations()
        .update_many(
            ["o1", "o2"]
                .map(|id| organization::ActiveModel {
                    id: Set(id.into()),
                    oauth_client_allowlist: Set(Some(json!([]))),
                    ..Default::default()
                })
                .into(),
        )
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, false, false, false]);
    // Removing one team removes that parent policy from the user's scope.
    store
        .team_members()
        .delete_many(&[("t1".into(), "u".into())])
        .await
        .unwrap();
    store
        .organizations()
        .update_many(vec![organization::ActiveModel {
            id: Set("o2".into()),
            oauth_client_allowlist: Set(None),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, true, false, false]);
    store
        .oauth_clients()
        .update_many(vec![client::ActiveModel {
            id: Set("b".into()),
            enabled: Set(false),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(allowed(&store).await, [false, false, false, false]);
}

fn authorization() -> Authorization {
    Authorization {
        api_key: api_key::ActiveModel {
            id: Set("key".into()),
            user_id: Set("u".into()),
            name: Set("oauth".into()),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set("internal-hash".into()),
            prefix: Set("oauth".into()),
            ..Default::default()
        },
        grant: grant::ActiveModel {
            id: Set("grant".into()),
            user_id: Set("u".into()),
            api_key_id: Set("key".into()),
            client_id: Set("a".into()),
            scopes: Set(json!(["gproxy"])),
            subject: Set("u".into()),
            created_at_ms: Set(1),
            ..Default::default()
        },
        code: code::ActiveModel {
            id: Set("code".into()),
            code_hash: Set(vec![1; 32]),
            grant_id: Set("grant".into()),
            redirect_uri: Set("http://localhost/callback".into()),
            code_challenge: Set("challenge".into()),
            created_at_ms: Set(1),
            expires_at_ms: Set(1000),
            ..Default::default()
        },
        device: Some(DeviceApproval {
            id: "device".into(),
            authorization_payload: None,
        }),
        now_ms: 1,
    }
}
fn issued(id: &str, hash: u8) -> IssuedToken {
    IssuedToken {
        id: id.into(),
        hash: vec![hash; 32],
        expires_at_ms: 1000,
    }
}

#[tokio::test]
async fn narrowing_blocks_device_approval_code_refresh_and_existing_access() {
    let store = database().await;
    store
        .oauth_devices()
        .create_many(vec![device::ActiveModel {
            id: Set("device".into()),
            client_id: Set("a".into()),
            device_code_hash: Set(vec![9; 32]),
            user_code: Set("CODE".into()),
            scopes: Set(json!(["gproxy"])),
            created_at_ms: Set(0),
            expires_at_ms: Set(1000),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(Some(json!(["b"]))),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .issue_many(vec![authorization()])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert!(store.api_keys().get_many(&["key".into()]).await.unwrap()[0].is_none());
    assert!(
        store
            .oauth_grants()
            .get_many(&["grant".into()])
            .await
            .unwrap()[0]
            .is_none()
    );
    assert!(
        store
            .oauth_codes()
            .get_many(&["code".into()])
            .await
            .unwrap()[0]
            .is_none()
    );
    let row = store
        .oauth_devices()
        .get_many(&["device".into()])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert!(row.approval_receipt.is_none() && row.approved_at_ms.is_none());
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(None),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .issue_many(vec![authorization()])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    let exchange = TokenExchange {
        source: ExchangeSource::Code {
            id: "code".into(),
            hash: vec![1; 32],
            redirect_uri: "http://localhost/callback".into(),
            code_challenge: "challenge".into(),
        },
        grant_id: "grant".into(),
        client_id: "a".into(),
        access: issued("access", 2),
        refresh: issued("refresh", 3),
        now_ms: 2,
    };
    store
        .users()
        .update_many(vec![user::ActiveModel {
            id: Set("u".into()),
            oauth_client_allowlist: Set(Some(json!([]))),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![exchange.clone()])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert!(
        store
            .oauth_codes()
            .get_many(&["code".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .consumed_at_ms
            .is_none()
    );
    assert!(
        store
            .oauth_tokens()
            .get_many(&["access".into()])
            .await
            .unwrap()[0]
            .is_none()
    );
    store
        .users()
        .update_many(vec![user::ActiveModel {
            id: Set("u".into()),
            oauth_client_allowlist: Set(None),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![exchange])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert!(
        store
            .oauth_grants()
            .resolve_access_many(&[vec![2; 32]], 3)
            .await
            .unwrap()[0]
            .is_some()
    );
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(Some(json!([]))),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        store
            .oauth_grants()
            .resolve_access_many(&[vec![2; 32]], 3)
            .await
            .unwrap()[0]
            .is_none()
    );
    let refresh = TokenExchange {
        source: ExchangeSource::Refresh {
            id: "refresh".into(),
            hash: vec![3; 32],
        },
        grant_id: "grant".into(),
        client_id: "a".into(),
        access: issued("access2", 4),
        refresh: issued("refresh2", 5),
        now_ms: 3,
    };
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![refresh.clone()])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert!(
        store
            .oauth_tokens()
            .get_many(&["refresh".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .consumed_at_ms
            .is_none()
    );
    assert_eq!(
        store
            .oauth_grants()
            .get_many(&["grant".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .refresh_count,
        0
    );
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(None),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![refresh])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
}
