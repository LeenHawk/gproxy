#![cfg(not(target_arch = "wasm32"))]
//! The identity snapshot against a real database: one `load_all_data()` read
//! becomes one `AppData`, and the indexes answer for the rows that were there.

use gproxy_app::{
    AppData, AppSnapshot,
    snapshot::{Decision, Owner, Subject, encode_key_hash},
};
use gproxy_protocol::Operation;
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        identity::{
            api_key, membership_role::MembershipRole, organization, organization_member,
            permission, team, team_member, user,
        },
        oauth,
        upstream::{credential, provider},
    },
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn digest_of(key: &str) -> [u8; 32] {
    Sha256::digest(key.as_bytes()).into()
}

/// An organization with a team, four users, three keys, a provider with one
/// credential per owner kind, and two permission rules.
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
            config_revision: Set(11),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![
            user::ActiveModel {
                id: Set("alice".into()),
                name: Set("alice".into()),
                role: Set("admin".into()),
                oauth_client_allowlist: Set(Some(json!(["codex"]))),
                created_at_ms: Set(0),
                ..Default::default()
            },
            user::ActiveModel {
                id: Set("bob".into()),
                name: Set("bob".into()),
                role: Set("user".into()),
                created_at_ms: Set(0),
                ..Default::default()
            },
            user::ActiveModel {
                id: Set("carol".into()),
                name: Set("carol".into()),
                role: Set("user".into()),
                created_at_ms: Set(0),
                ..Default::default()
            },
            user::ActiveModel {
                id: Set("banned".into()),
                name: Set("banned".into()),
                role: Set("user".into()),
                enabled: Set(false),
                created_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .organizations()
        .create_many(vec![organization::ActiveModel {
            id: Set("acme".into()),
            name: Set("acme".into()),
            oauth_client_allowlist: Set(Some(json!(["codex", "claude"]))),
            created_at_ms: Set(0),
        }])
        .await
        .unwrap();
    store
        .teams()
        .create_many(vec![team::ActiveModel {
            id: Set("core".into()),
            organization_id: Set("acme".into()),
            name: Set("core".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .organization_members()
        .create_many(vec![organization_member::ActiveModel {
            organization_id: Set("acme".into()),
            user_id: Set("alice".into()),
            role: Set(MembershipRole::Admin),
        }])
        .await
        .unwrap();
    store
        .team_members()
        .create_many(vec![team_member::ActiveModel {
            team_id: Set("core".into()),
            user_id: Set("bob".into()),
            role: Set(MembershipRole::Member),
        }])
        .await
        .unwrap();
    store
        .api_keys()
        .create_many(vec![
            api_key::ActiveModel {
                id: Set("k-alice".into()),
                user_id: Set("alice".into()),
                organization_id: Set(Some("acme".into())),
                name: Set("alice".into()),
                key_hash: Set(encode_key_hash(&digest_of("sk-alice"))),
                prefix: Set("sk-".into()),
                ..Default::default()
            },
            api_key::ActiveModel {
                id: Set("k-bob".into()),
                user_id: Set("bob".into()),
                team_id: Set(Some("core".into())),
                name: Set("bob".into()),
                key_hash: Set(encode_key_hash(&digest_of("sk-bob"))),
                prefix: Set("sk-".into()),
                ..Default::default()
            },
            api_key::ActiveModel {
                id: Set("k-banned".into()),
                user_id: Set("banned".into()),
                name: Set("banned".into()),
                key_hash: Set(encode_key_hash(&digest_of("sk-banned"))),
                prefix: Set("sk-".into()),
                ..Default::default()
            },
            api_key::ActiveModel {
                id: Set("k-off".into()),
                user_id: Set("carol".into()),
                name: Set("off".into()),
                key_hash: Set(encode_key_hash(&digest_of("sk-off"))),
                prefix: Set("sk-".into()),
                enabled: Set(false),
                ..Default::default()
            },
            // A digest written before this crate fixed the encoding.
            api_key::ActiveModel {
                id: Set("k-legacy".into()),
                user_id: Set("carol".into()),
                name: Set("legacy".into()),
                key_hash: Set("not-a-digest".into()),
                prefix: Set("sk-".into()),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .permissions()
        .create_many(vec![
            permission::ActiveModel {
                id: Set("p-allow".into()),
                user_id: Set(Some("alice".into())),
                model_pattern: Set("*".into()),
                action: Set("allow".into()),
                priority: Set(0),
                ..Default::default()
            },
            permission::ActiveModel {
                id: Set("p-deny".into()),
                user_id: Set(Some("alice".into())),
                model_pattern: Set("gpt-*".into()),
                action: Set("deny".into()),
                priority: Set(10),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("openai".into()),
            name: Set("openai".into()),
            channel: Set("custom".into()),
            config: Set(json!({})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let credential = |id: &str, user: Option<&str>, team: Option<&str>, org: Option<&str>| {
        credential::ActiveModel {
            id: Set(id.into()),
            provider_id: Set("openai".into()),
            user_id: Set(user.map(Into::into)),
            team_id: Set(team.map(Into::into)),
            organization_id: Set(org.map(Into::into)),
            auth_kind: Set("api_key".into()),
            secret: Set(Vec::new()),
            metadata: Set(json!({})),
            ..Default::default()
        }
    };
    store
        .credentials()
        .create_many(vec![
            credential("c-shared", None, None, None),
            credential("c-alice", Some("alice"), None, None),
            credential("c-core", None, Some("core"), None),
            credential("c-acme", None, None, Some("acme")),
        ])
        .await
        .unwrap();
    store
        .oauth_clients()
        .create_many(vec![oauth::client::ActiveModel {
            id: Set("codex".into()),
            name: Set("Codex CLI".into()),
            redirect_uris: Set(json!(["http://localhost:1455/callback"])),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
}

async fn assembled() -> AppData {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    let revision = all.control.settings.as_ref().unwrap().config_revision;
    AppData::assemble(revision, &all.identity, &all.control.credentials).unwrap()
}

#[tokio::test]
async fn one_read_assembles_the_whole_identity_layer() {
    let data = assembled().await;
    assert_eq!(data.revision, 11);
    assert_eq!(data.users.len(), 4);
    assert_eq!(data.organizations.len(), 1);
    assert_eq!(data.teams.len(), 1);
    assert_eq!(data.oauth_clients.len(), 1);
    assert_eq!(data.credential_ownership.len(), 4);
}

#[tokio::test]
async fn keys_resolve_to_their_owner_and_binding() {
    let data = assembled().await;
    let alice = data.keys.lookup(&digest_of("sk-alice")).unwrap();
    assert_eq!(alice.api_key_id, "k-alice");
    assert_eq!(alice.user_id, "alice");
    assert_eq!(alice.user_role, "admin");
    assert_eq!(alice.organization_id.as_deref(), Some("acme"));
    assert_eq!(alice.team_id, None);

    let bob = data.keys.lookup(&digest_of("sk-bob")).unwrap();
    assert_eq!(bob.team_id.as_deref(), Some("core"));
    assert_eq!(bob.organization_id, None);
    // A team-bound key reaches its parent organization through the team.
    assert_eq!(
        data.effective_organization(bob.organization_id.as_deref(), bob.team_id.as_deref()),
        Some("acme")
    );
}

#[tokio::test]
async fn unusable_key_rows_never_reach_the_index() {
    let data = assembled().await;
    assert_eq!(data.keys.len(), 2, "alice and bob only");
    assert!(data.keys.lookup(&digest_of("sk-banned")).is_none());
    assert!(data.keys.lookup(&digest_of("sk-off")).is_none());
    assert!(data.keys.lookup(&digest_of("sk-unknown")).is_none());
}

#[tokio::test]
async fn memberships_carry_their_roles_and_parents() {
    let data = assembled().await;
    assert!(data.memberships.is_admin_of_org("alice", "acme"));
    assert!(!data.memberships.is_admin_of_team("alice", "core"));
    assert_eq!(
        data.memberships.role_in_team("bob", "core"),
        Some(MembershipRole::Member)
    );
    assert_eq!(data.memberships.parent_of("core"), Some("acme"));
    assert_eq!(
        data.memberships.effective_organizations("bob"),
        vec!["acme"]
    );
}

#[tokio::test]
async fn credential_visibility_follows_the_key_binding() {
    let data = assembled().await;
    assert_eq!(
        data.credential_ownership.owner("c-core"),
        Some(&Owner::Team("core".into()))
    );
    // alice: her own, the organization's and the shared one.
    assert_eq!(
        data.credential_ownership
            .visible("alice", None, Some("acme")),
        vec!["c-acme", "c-alice", "c-shared"]
    );
    // bob through the team, plus its parent organization.
    assert_eq!(
        data.credential_ownership
            .visible("bob", Some("core"), Some("acme")),
        vec!["c-acme", "c-core", "c-shared"]
    );
    // A caller bound to nothing sees only what is shared.
    assert_eq!(
        data.credential_ownership.visible("carol", None, None),
        vec!["c-shared"]
    );
}

#[tokio::test]
async fn permissions_decide_with_the_priority_they_were_stored_with() {
    let data = assembled().await;
    let alice = Subject::key("alice", "k-alice");
    assert_eq!(
        data.permissions.decide(
            &alice,
            "openai",
            Some("claude-3"),
            Operation::GenerateContent
        ),
        Decision::Allow
    );
    assert!(matches!(
        data.permissions
            .decide(&alice, "openai", Some("gpt-4o"), Operation::GenerateContent),
        Decision::Deny(reason) if reason.contains("p-deny")
    ));
    // bob has no rule at all.
    let bob = Subject::key("bob", "k-bob");
    assert!(
        data.permissions
            .allowed_providers(&bob, Some("gpt-4o"), Operation::GenerateContent, ["openai"])
            .is_empty()
    );
}

#[tokio::test]
async fn the_oauth_allowlist_intersects_the_user_and_its_organization() {
    let data = assembled().await;
    let orgs = data.memberships.effective_organizations("alice");
    let teams: Vec<String> = Vec::new();
    assert!(
        data.oauth_client_allowlist
            .allowed("alice", &orgs, &teams, "codex")
    );
    // Allowed by acme, not by alice.
    assert!(
        !data
            .oauth_client_allowlist
            .allowed("alice", &orgs, &teams, "claude")
    );
    // bob restricts nothing of his own and inherits acme's list.
    let bob_orgs = data.memberships.effective_organizations("bob");
    let bob_teams = vec!["core".to_string()];
    assert!(
        data.oauth_client_allowlist
            .allowed("bob", &bob_orgs, &bob_teams, "claude")
    );
    assert!(
        !data
            .oauth_client_allowlist
            .allowed("bob", &bob_orgs, &bob_teams, "gemini")
    );
}

#[tokio::test]
async fn a_snapshot_only_moves_forward() {
    let store = seeded().await;
    let all = store.load_all_data().await.unwrap();
    let current = Arc::new(AppData::assemble(11, &all.identity, &all.control.credentials).unwrap());
    let snapshot = AppSnapshot::default();
    assert!(snapshot.publish_if_newer(current));
    assert_eq!(snapshot.revision(), 11);

    // A slow reload that started earlier must not resurrect deleted rows.
    let stale = Arc::new(AppData::assemble(10, &all.identity, &[]).unwrap());
    assert!(!snapshot.publish_if_newer(stale));
    assert_eq!(snapshot.load().credential_ownership.len(), 4);

    let next = Arc::new(AppData::assemble(12, &all.identity, &[]).unwrap());
    assert!(snapshot.publish_if_newer(next));
    assert_eq!(snapshot.revision(), 12);
    assert_eq!(snapshot.load().credential_ownership.len(), 0);
}
