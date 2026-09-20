#![cfg(not(target_arch = "wasm32"))]
//! The in-memory OAuth client allowlist against the SQL one.
//!
//! `gproxy-app` evaluates the policy from the snapshot and `gproxy-store`
//! evaluates the same policy inside the statement that performs the write.
//! Two implementations of one rule drift, so this pins them together over a
//! matrix that exercises every level and both membership paths.

use gproxy_app::AppData;
use gproxy_store::{
    Store,
    entity::{
        identity::{
            membership_role::MembershipRole, organization, organization_member, team, team_member,
            user,
        },
        oauth,
    },
    operations::oauth::ClientAccess,
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::{Value, json};

const CLIENTS: [&str; 3] = ["a", "b", "c"];
const USERS: [&str; 5] = ["u-open", "u-empty", "u-a", "u-team", "u-multi"];

fn user_row(id: &str, allowlist: Option<Value>) -> user::ActiveModel {
    user::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        role: Set("user".into()),
        oauth_client_allowlist: Set(allowlist),
        created_at_ms: Set(0),
        ..Default::default()
    }
}

/// Three organizations, two teams, five users covering: nothing configured,
/// an empty list, a user list narrowed by an organization, a team-only
/// membership reaching its parent organization, and two organizations whose
/// lists union.
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
        .users()
        .create_many(vec![
            user_row("u-open", None),
            user_row("u-empty", Some(json!([]))),
            user_row("u-a", Some(json!(["a", "b"]))),
            user_row("u-team", None),
            user_row("u-multi", None),
        ])
        .await
        .unwrap();
    store
        .organizations()
        .create_many(vec![
            organization::ActiveModel {
                id: Set("o-none".into()),
                name: Set("o-none".into()),
                oauth_client_allowlist: Set(None),
                created_at_ms: Set(0),
            },
            organization::ActiveModel {
                id: Set("o-a".into()),
                name: Set("o-a".into()),
                oauth_client_allowlist: Set(Some(json!(["a"]))),
                created_at_ms: Set(0),
            },
            organization::ActiveModel {
                id: Set("o-b".into()),
                name: Set("o-b".into()),
                oauth_client_allowlist: Set(Some(json!(["b"]))),
                created_at_ms: Set(0),
            },
        ])
        .await
        .unwrap();
    store
        .teams()
        .create_many(vec![
            team::ActiveModel {
                id: Set("t-plain".into()),
                organization_id: Set("o-none".into()),
                name: Set("t-plain".into()),
                oauth_client_allowlist: Set(None),
                created_at_ms: Set(0),
            },
            team::ActiveModel {
                id: Set("t-ab".into()),
                organization_id: Set("o-b".into()),
                name: Set("t-ab".into()),
                oauth_client_allowlist: Set(Some(json!(["a", "b"]))),
                created_at_ms: Set(0),
            },
        ])
        .await
        .unwrap();
    store
        .organization_members()
        .create_many(vec![
            organization_member::ActiveModel {
                organization_id: Set("o-a".into()),
                user_id: Set("u-a".into()),
                role: Set(MembershipRole::Member),
            },
            organization_member::ActiveModel {
                organization_id: Set("o-a".into()),
                user_id: Set("u-multi".into()),
                role: Set(MembershipRole::Member),
            },
            organization_member::ActiveModel {
                organization_id: Set("o-b".into()),
                user_id: Set("u-multi".into()),
                role: Set(MembershipRole::Member),
            },
        ])
        .await
        .unwrap();
    store
        .team_members()
        .create_many(vec![
            team_member::ActiveModel {
                team_id: Set("t-ab".into()),
                user_id: Set("u-team".into()),
                role: Set(MembershipRole::Member),
            },
            team_member::ActiveModel {
                team_id: Set("t-plain".into()),
                user_id: Set("u-open".into()),
                role: Set(MembershipRole::Member),
            },
        ])
        .await
        .unwrap();
    store
        .oauth_clients()
        .create_many(
            CLIENTS
                .map(|id| oauth::client::ActiveModel {
                    id: Set(id.into()),
                    name: Set(id.into()),
                    redirect_uris: Set(json!([])),
                    // `c` is registered but switched off, which only the store
                    // checks; the app filters live clients separately.
                    enabled: Set(id != "c"),
                    ..Default::default()
                })
                .into(),
        )
        .await
        .unwrap();
    store
}

#[tokio::test]
async fn the_snapshot_answers_what_the_sql_policy_answers() {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    let data = AppData::assemble(0, &all.identity, &all.control.credentials).unwrap();

    let requests: Vec<ClientAccess> = USERS
        .iter()
        .flat_map(|user| {
            CLIENTS.iter().map(move |client| ClientAccess {
                user_id: (*user).into(),
                client_id: (*client).into(),
            })
        })
        .collect();
    let from_sql = store.oauth_clients().allowed_many(&requests).await.unwrap();

    for (request, sql) in requests.iter().zip(from_sql) {
        let organizations = data.memberships.effective_organizations(&request.user_id);
        let teams: Vec<String> = data
            .memberships
            .teams_of(&request.user_id)
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        // The store also requires the client to be live; the app's own
        // `usable_clients` applies the same filter.
        let live = data
            .oauth_clients
            .get(&request.client_id)
            .is_some_and(|client| client.enabled && client.deleted_at_ms.is_none());
        let app = live
            && data.oauth_client_allowlist.allowed(
                &request.user_id,
                &organizations,
                &teams,
                &request.client_id,
            );
        assert_eq!(
            app, sql,
            "user {} client {}",
            request.user_id, request.client_id
        );
    }
}

#[tokio::test]
async fn the_matrix_it_agrees_on_is_the_one_that_was_intended() {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    let data = AppData::assemble(0, &all.identity, &all.control.credentials).unwrap();

    let usable = |user: &str| {
        let organizations = data.memberships.effective_organizations(user);
        let teams: Vec<String> = data
            .memberships
            .teams_of(user)
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
        let mut clients: Vec<&oauth::client::Model> = data.oauth_clients.values().collect();
        clients.sort_by(|left, right| left.id.cmp(&right.id));
        data.oauth_client_allowlist
            .usable_clients(user, &organizations, &teams, clients)
    };

    // Nothing configured on the user, its team or that team's organization.
    // Only `c` is missing, because it is switched off.
    assert_eq!(usable("u-open"), vec!["a", "b"]);
    // An empty list on the user denies everything below it.
    assert!(usable("u-empty").is_empty());
    // The user allows a and b; its organization narrows that to a.
    assert_eq!(usable("u-a"), vec!["a"]);
    // A team-only membership is still held to the team's parent organization,
    // which allows b; the team itself allows a and b.
    assert_eq!(usable("u-team"), vec!["b"]);
    // Two configured organizations union within their level.
    assert_eq!(usable("u-multi"), vec!["a", "b"]);
}
