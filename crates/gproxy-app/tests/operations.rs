#![cfg(not(target_arch = "wasm32"))]
//! The identity operation families against a real database.
//!
//! Every test here drives a real `Gproxy` handle over in-memory SQLite, so a
//! write goes through `Store::commit_revision` exactly as it would in
//! production: the rows and the revision bump are one batch, and a create
//! reads its row back from inside that batch.
//!
//! Three properties are asserted over and over, because they are the contract
//! rather than the implementation:
//!
//! 1. an accepted write moves `settings.config_revision` by **exactly one**;
//! 2. a rejected write moves it by **zero**;
//! 3. a read of the written row answers with what the write returned.

use gproxy_app::{
    AppConfig, AppData, Authenticator, Operations,
    audit::{AuditEntry, REDACTED},
    dto::{
        ApiKeyPatch, ApiKeyWrite, AuditQuery, ListQuery, MemberPatch, MemberWrite,
        OAuthClientWrite, OrganizationPatch, OrganizationWrite, PermissionWrite, RateLimitPatch,
        RateLimitWrite, TeamWrite, UserPatch, UserWrite,
    },
};
use gproxy_cache::{MemoryCache, MemoryOptions};
use gproxy_sdk::{Gproxy, GproxyBuilder, SyncMode};
use gproxy_store::{
    Store,
    entity::{
        identity::{api_key, organization, team, user, user_session},
        oauth,
        upstream::provider,
    },
};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// A handle over a private in-memory database, with plaintext secret sealing
/// so a test can assert on what `reveal` returns without a master key.
async fn handle() -> Gproxy<DatabaseConnection> {
    GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .cache(Arc::new(
            MemoryCache::new(MemoryOptions::default()).unwrap(),
        ))
        // Nothing in this file wants a background poller deciding when a
        // snapshot advances.
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap()
}

/// The identity snapshot of the current durable revision.
async fn snapshot(store: &Store<DatabaseConnection>) -> AppData {
    let all = store.load_all_data().await.unwrap();
    let revision = revision(store).await;
    AppData::assemble(revision, &all.identity, &all.control.credentials).unwrap()
}

async fn revision(store: &Store<DatabaseConnection>) -> i64 {
    store
        .settings()
        .get()
        .await
        .unwrap()
        .expect("the builder creates the settings row")
        .config_revision
}

/// One instance-wide administrator, so the last-admin guard never fires by
/// accident in tests that are about something else.
async fn seed_admin(gproxy: &Gproxy<DatabaseConnection>) -> String {
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    Operations::new(gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "root".into(),
            role: Some("admin".into()),
            ..UserWrite::default()
        })
        .await
        .unwrap()
        .id
}

/// Run `body` with an `Operations` built over a freshly loaded snapshot, and
/// assert the revision moved by `expected`.
macro_rules! moves_revision {
    ($gproxy:expr, $expected:expr, $body:expr) => {{
        let before = revision($gproxy.store()).await;
        let data = snapshot($gproxy.store()).await;
        let config = AppConfig::default();
        let operations = Operations::new(&$gproxy, &data, &config);
        let outcome = $body(operations).await;
        let after = revision($gproxy.store()).await;
        assert_eq!(
            after - before,
            $expected,
            "revision moved by {} rather than {}",
            after - before,
            $expected
        );
        outcome
    }};
}

// ---------------------------------------------------------------------------
// Every family's round trip.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn users_round_trip_and_every_write_is_one_revision() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;

    let created = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .create(UserWrite {
                name: "  alice  ".into(),
                password: Some("correct horse battery staple".into()),
                ..UserWrite::default()
            })
            .await
            .unwrap()
    });
    // The name is trimmed, the role defaults, and the hash never comes back.
    assert_eq!(created.name, "alice");
    assert_eq!(created.role, "user");
    assert!(created.enabled);
    assert!(created.has_password);

    let listed = moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        operations
            .users()
            .list(ListQuery {
                search: Some("alic".into()),
                ..ListQuery::default()
            })
            .await
            .unwrap()
    });
    assert_eq!(listed.total, 1);
    assert_eq!(listed.items[0].id, created.id);

    let id = created.id.clone();
    let updated = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .update(
                &id,
                UserPatch {
                    name: Some("alice-renamed".into()),
                    oauth_client_allowlist: Some(Some(vec!["cli".into(), "cli".into()])),
                    ..UserPatch::default()
                },
            )
            .await
            .unwrap()
    });
    assert_eq!(updated.name, "alice-renamed");
    // Deduplicated in the order it was given.
    assert_eq!(updated.oauth_client_allowlist, Some(vec!["cli".into()]));

    // `null` clears the allowlist back to inheriting.
    let id = created.id.clone();
    let cleared = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.users().set_allowlist(&id, None).await.unwrap()
    });
    assert_eq!(cleared.oauth_client_allowlist, None);

    let id = created.id.clone();
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.users().delete(&id).await.unwrap()
    });
    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let error = Operations::new(&gproxy, &data, &config)
        .users()
        .get(&created.id)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 404);
}

#[tokio::test]
async fn a_rejected_write_does_not_move_the_revision() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;

    // A duplicate name, a blank name and an unknown role are all refused
    // before anything is committed.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .users()
            .create(UserWrite {
                name: "root".into(),
                ..UserWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 409);
    });
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .users()
            .create(UserWrite {
                name: "   ".into(),
                ..UserWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .users()
            .create(UserWrite {
                name: "mallory".into(),
                role: Some("superuser".into()),
                ..UserWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
    // And a short password is refused before the row is built, so the user is
    // not created without one.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .users()
            .create(UserWrite {
                name: "mallory".into(),
                password: Some("short".into()),
                ..UserWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
}

#[tokio::test]
async fn the_last_enabled_administrator_cannot_be_removed_disabled_or_demoted() {
    let gproxy = handle().await;
    let root = seed_admin(&gproxy).await;

    for outcome in [
        moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
            operations.users().delete(&root).await
        }),
        moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
            operations
                .users()
                .update(
                    &root,
                    UserPatch {
                        enabled: Some(false),
                        ..UserPatch::default()
                    },
                )
                .await
                .map(|_| ())
        }),
        moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
            operations
                .users()
                .update(
                    &root,
                    UserPatch {
                        role: Some("user".into()),
                        ..UserPatch::default()
                    },
                )
                .await
                .map(|_| ())
        }),
    ] {
        assert_eq!(outcome.unwrap_err().status_code(), 409);
    }

    // A second administrator lifts the guard from the first.
    let second = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .create(UserWrite {
                name: "second-admin".into(),
                role: Some("admin".into()),
                ..UserWrite::default()
            })
            .await
            .unwrap()
    });
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.users().delete(&root).await.unwrap()
    });
    // And now the second one is the last one.
    let outcome = moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        operations.users().delete(&second.id).await
    });
    assert_eq!(outcome.unwrap_err().status_code(), 409);
}

#[tokio::test]
async fn a_password_change_rehashes_and_ends_every_session() {
    let gproxy = handle().await;
    let root = seed_admin(&gproxy).await;
    let config = AppConfig::default();

    // Two sessions, opened the way authentication opens them.
    let data = snapshot(gproxy.store()).await;
    let authenticator = Authenticator::new(gproxy.store(), &data, &config);
    let first = authenticator.create_session(&root, 1_000).await.unwrap();
    authenticator.create_session(&root, 1_000).await.unwrap();

    let sessions = Operations::new(&gproxy, &data, &config)
        .sessions()
        .list(&root)
        .await
        .unwrap();
    assert_eq!(sessions.total, 2);
    // Never the digest, never a token.
    let rendered = serde_json::to_string(&sessions.items[0]).unwrap();
    assert!(!rendered.contains("token"), "{rendered}");

    let user = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .set_password(&root, "a brand new passphrase")
            .await
            .unwrap()
    });
    assert!(user.has_password);

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    assert_eq!(operations.sessions().list(&root).await.unwrap().total, 0);
    // The token that was live a moment ago no longer resolves.
    let error = Authenticator::new(gproxy.store(), &data, &config)
        .authenticate_session(&first.token, 2_000)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);

    // Session revocation is not a configuration write.
    let second = Authenticator::new(gproxy.store(), &data, &config)
        .create_session(&root, 3_000)
        .await
        .unwrap();
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        operations.sessions().revoke(&second.id).await.unwrap();
        assert_eq!(operations.sessions().revoke_all(&root).await.unwrap(), 0);
        let error = operations.sessions().revoke("nope").await.unwrap_err();
        assert_eq!(error.status_code(), 404);
    });
}

#[tokio::test]
async fn organizations_teams_and_memberships_round_trip() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;

    let alice = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .create(UserWrite {
                name: "alice".into(),
                ..UserWrite::default()
            })
            .await
            .unwrap()
    });
    let org = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .organizations()
            .create(OrganizationWrite {
                name: "acme".into(),
                ..OrganizationWrite::default()
            })
            .await
            .unwrap()
    });
    let team = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .teams()
            .create(TeamWrite {
                organization_id: org.id.clone(),
                name: "platform".into(),
                ..TeamWrite::default()
            })
            .await
            .unwrap()
    });
    assert_eq!(team.organization_id, org.id);

    // A team under an organization that does not exist is a 404, not a
    // foreign-key error.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .teams()
            .create(TeamWrite {
                organization_id: "missing".into(),
                name: "ghosts".into(),
                ..TeamWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 404);
    });

    // The same team name in another organization is fine; in this one it is a
    // conflict.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .teams()
            .create(TeamWrite {
                organization_id: org.id.clone(),
                name: "platform".into(),
                ..TeamWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 409);
    });

    let member = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .members()
            .add(
                &org.id,
                MemberWrite {
                    user_id: alice.id.clone(),
                    ..MemberWrite::default()
                },
            )
            .await
            .unwrap()
    });
    assert_eq!(member.role, "member");

    let promoted = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .members()
            .set_role(
                &org.id,
                &alice.id,
                MemberPatch {
                    role: "admin".into(),
                },
            )
            .await
            .unwrap()
    });
    assert_eq!(promoted.role, "admin");

    // Adding the same member twice is a conflict rather than a silent no-op.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .members()
            .add(
                &org.id,
                MemberWrite {
                    user_id: alice.id.clone(),
                    ..MemberWrite::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 409);
    });

    let team_member = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .team_members()
            .add(
                &team.id,
                MemberWrite {
                    user_id: alice.id.clone(),
                    role: Some("admin".into()),
                },
            )
            .await
            .unwrap()
    });
    assert_eq!(team_member.role, "admin");

    let listed = moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        operations
            .members()
            .list(ListQuery {
                organization_id: Some(org.id.clone()),
                ..ListQuery::default()
            })
            .await
            .unwrap()
    });
    assert_eq!(listed.total, 1);

    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .team_members()
            .remove(&team.id, &alice.id)
            .await
            .unwrap();
    });
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .members()
            .remove(&org.id, &alice.id)
            .await
            .unwrap();
    });
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .members()
            .remove(&org.id, &alice.id)
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 404);
    });

    // Renaming keeps the parent; there is no field to move it.
    let renamed = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .organizations()
            .update(
                &org.id,
                OrganizationPatch {
                    name: Some("acme-two".into()),
                    ..OrganizationPatch::default()
                },
            )
            .await
            .unwrap()
    });
    assert_eq!(renamed.name, "acme-two");
}

// ---------------------------------------------------------------------------
// API keys.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_minted_key_authenticates_reveals_rotates_and_the_old_text_stops_working() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let alice = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .users()
            .create(UserWrite {
                name: "alice".into(),
                ..UserWrite::default()
            })
            .await
            .unwrap()
    });

    let minted = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .api_keys()
            .create(ApiKeyWrite {
                user_id: alice.id.clone(),
                name: "laptop".into(),
                retain_secret: Some(true),
                ..ApiKeyWrite::default()
            })
            .await
            .unwrap()
    });
    assert!(minted.token.starts_with("sk-"));
    assert!(minted.key.has_secret);
    assert_eq!(minted.key.kind, "user");
    // The display prefix is the head of the body, not a secret.
    assert_eq!(minted.key.prefix, minted.token["sk-".len()..][..8]);
    // Nothing in the DTO carries the digest or the sealed bytes.
    let rendered = serde_json::to_string(&minted.key).unwrap();
    assert!(!rendered.contains("keyHash"), "{rendered}");
    assert!(!rendered.contains("secret\":["), "{rendered}");
    // The row holds the digest of the whole text and nothing else.
    let stored = api_key::Entity::find_by_id(minted.key.id.clone())
        .one(gproxy.store().connection())
        .await
        .unwrap()
        .unwrap();
    let digest: [u8; 32] = Sha256::digest(minted.token.as_bytes()).into();
    assert_eq!(
        stored.key_hash,
        gproxy_app::snapshot::encode_key_hash(&digest)
    );
    assert_ne!(stored.key_hash, minted.token);

    // It authenticates through the P2 ladder against the new snapshot.
    let data = snapshot(gproxy.store()).await;
    let caller = Authenticator::new(gproxy.store(), &data, &config)
        .authenticate_token(&minted.token)
        .await
        .unwrap();
    assert_eq!(caller.user_id, alice.id);
    assert_eq!(caller.api_key_id.as_deref(), Some(minted.key.id.as_str()));

    // Reveal answers with exactly the text that was minted.
    let revealed = moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        operations.api_keys().reveal(&minted.key.id).await.unwrap()
    });
    assert_eq!(revealed.token, minted.token);

    // Rotation is one revision, and the old text stops authenticating.
    let rotated = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.api_keys().rotate(&minted.key.id).await.unwrap()
    });
    assert_ne!(rotated.token, minted.token);
    assert_eq!(rotated.key.id, minted.key.id);
    assert!(rotated.key.has_secret);

    let data = snapshot(gproxy.store()).await;
    let authenticator = Authenticator::new(gproxy.store(), &data, &config);
    let error = authenticator
        .authenticate_token(&minted.token)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 401);
    let caller = authenticator
        .authenticate_token(&rotated.token)
        .await
        .unwrap();
    assert_eq!(caller.api_key_id.as_deref(), Some(minted.key.id.as_str()));
    assert_eq!(
        Operations::new(&gproxy, &data, &config)
            .api_keys()
            .reveal(&minted.key.id)
            .await
            .unwrap()
            .token,
        rotated.token
    );
}

#[tokio::test]
async fn a_key_that_retained_nothing_cannot_be_revealed() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let alice = operations
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let minted = operations
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "ephemeral".into(),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    assert!(!minted.key.has_secret);

    let data = snapshot(gproxy.store()).await;
    let error = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .reveal(&minted.key.id)
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 409);

    // Rotating one that retained nothing still retains nothing.
    let data = snapshot(gproxy.store()).await;
    let rotated = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .rotate(&minted.key.id)
        .await
        .unwrap();
    assert!(!rotated.key.has_secret);
}

#[tokio::test]
async fn a_key_binding_is_checked_against_membership_and_the_team_parent() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let alice = operations
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let acme = operations
        .organizations()
        .create(OrganizationWrite {
            name: "acme".into(),
            ..OrganizationWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let other = operations
        .organizations()
        .create(OrganizationWrite {
            name: "other".into(),
            ..OrganizationWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let team = operations
        .teams()
        .create(TeamWrite {
            organization_id: acme.id.clone(),
            name: "platform".into(),
            ..TeamWrite::default()
        })
        .await
        .unwrap();

    // Not a member yet: the binding is refused.
    let before = revision(gproxy.store()).await;
    let data = snapshot(gproxy.store()).await;
    let error = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "scoped".into(),
            organization_id: Some(acme.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 400);
    assert_eq!(revision(gproxy.store()).await, before);

    // Make her a member of both scopes.
    for (scope, _) in [(acme.id.clone(), ()), (other.id.clone(), ())] {
        let data = snapshot(gproxy.store()).await;
        Operations::new(&gproxy, &data, &config)
            .members()
            .add(
                &scope,
                MemberWrite {
                    user_id: alice.id.clone(),
                    ..MemberWrite::default()
                },
            )
            .await
            .unwrap();
    }
    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .team_members()
        .add(
            &team.id,
            MemberWrite {
                user_id: alice.id.clone(),
                ..MemberWrite::default()
            },
        )
        .await
        .unwrap();

    // A team bound under the wrong organization is refused even though the
    // user is a member of both.
    let before = revision(gproxy.store()).await;
    let data = snapshot(gproxy.store()).await;
    let error = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "mismatched".into(),
            organization_id: Some(other.id.clone()),
            team_id: Some(team.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 400);
    assert_eq!(revision(gproxy.store()).await, before);

    // The matching pair is accepted.
    let data = snapshot(gproxy.store()).await;
    let minted = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "scoped".into(),
            organization_id: Some(acme.id.clone()),
            team_id: Some(team.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    assert_eq!(
        minted.key.organization_id.as_deref(),
        Some(acme.id.as_str())
    );

    // A patch is re-validated against the binding the row will hold, not just
    // the half it names: moving only the organization breaks the pair.
    let before = revision(gproxy.store()).await;
    let data = snapshot(gproxy.store()).await;
    let error = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .update(
            &minted.key.id,
            ApiKeyPatch {
                organization_id: Some(Some(other.id.clone())),
                ..ApiKeyPatch::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 400);
    assert_eq!(revision(gproxy.store()).await, before);

    // Clearing the team and the organization together is fine.
    let data = snapshot(gproxy.store()).await;
    let unbound = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .update(
            &minted.key.id,
            ApiKeyPatch {
                organization_id: Some(None),
                team_id: Some(None),
                ..ApiKeyPatch::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(unbound.organization_id, None);
    assert_eq!(unbound.team_id, None);
}

#[tokio::test]
async fn an_oauth_key_is_not_a_bearer_key_and_cannot_be_revealed_or_rotated() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    // The issuer writes this row, not this family.
    gproxy
        .store()
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("grant-key".into()),
            user_id: Set(alice.id),
            name: Set("grant".into()),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set("00".repeat(32)),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    assert_eq!(
        operations
            .api_keys()
            .reveal("grant-key")
            .await
            .unwrap_err()
            .status_code(),
        409
    );
    assert_eq!(
        operations
            .api_keys()
            .rotate("grant-key")
            .await
            .unwrap_err()
            .status_code(),
        409
    );
    // It is still listed, so an operator can see it exists.
    assert_eq!(
        operations.api_keys().get("grant-key").await.unwrap().kind,
        "oauth"
    );
}

// ---------------------------------------------------------------------------
// The explicit cascade.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn deleting_an_organization_removes_its_teams_and_every_key_bound_to_either() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let org = Operations::new(&gproxy, &data, &config)
        .organizations()
        .create(OrganizationWrite {
            name: "acme".into(),
            ..OrganizationWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let team = Operations::new(&gproxy, &data, &config)
        .teams()
        .create(TeamWrite {
            organization_id: org.id.clone(),
            name: "platform".into(),
            ..TeamWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .members()
        .add(
            &org.id,
            MemberWrite {
                user_id: alice.id.clone(),
                ..MemberWrite::default()
            },
        )
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .team_members()
        .add(
            &team.id,
            MemberWrite {
                user_id: alice.id.clone(),
                ..MemberWrite::default()
            },
        )
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let org_key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "org".into(),
            organization_id: Some(org.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let team_key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "team".into(),
            team_id: Some(team.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let loose_key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "personal".into(),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();

    // The whole cascade is one revision: the two keys, the team and the
    // organization land in a single batch.
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.organizations().delete(&org.id).await.unwrap();
    });

    let connection = gproxy.store().connection();
    assert!(
        organization::Entity::find_by_id(org.id.clone())
            .one(connection)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        team::Entity::find_by_id(team.id.clone())
            .one(connection)
            .await
            .unwrap()
            .is_none()
    );
    for gone in [&org_key.key.id, &team_key.key.id] {
        assert!(
            api_key::Entity::find_by_id(gone.clone())
                .one(connection)
                .await
                .unwrap()
                .is_none(),
            "key {gone} outlived the scope it was bound to"
        );
    }
    // A key bound to nothing is untouched: the cascade follows the binding,
    // not the user.
    assert!(
        api_key::Entity::find_by_id(loose_key.key.id.clone())
            .one(connection)
            .await
            .unwrap()
            .is_some()
    );
    // And the user is still there.
    assert!(
        user::Entity::find_by_id(alice.id)
            .one(connection)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn deleting_a_team_removes_only_the_keys_bound_to_it() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let org = Operations::new(&gproxy, &data, &config)
        .organizations()
        .create(OrganizationWrite {
            name: "acme".into(),
            ..OrganizationWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let team = Operations::new(&gproxy, &data, &config)
        .teams()
        .create(TeamWrite {
            organization_id: org.id.clone(),
            name: "platform".into(),
            ..TeamWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .members()
        .add(
            &org.id,
            MemberWrite {
                user_id: alice.id.clone(),
                ..MemberWrite::default()
            },
        )
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .team_members()
        .add(
            &team.id,
            MemberWrite {
                user_id: alice.id.clone(),
                ..MemberWrite::default()
            },
        )
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let team_key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "team".into(),
            team_id: Some(team.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let org_key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "org".into(),
            organization_id: Some(org.id.clone()),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();

    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.teams().delete(&team.id).await.unwrap();
    });

    let connection = gproxy.store().connection();
    assert!(
        api_key::Entity::find_by_id(team_key.key.id)
            .one(connection)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        api_key::Entity::find_by_id(org_key.key.id)
            .one(connection)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        organization::Entity::find_by_id(org.id)
            .one(connection)
            .await
            .unwrap()
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// Permissions and rate limits.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn permissions_round_trip_and_refuse_a_rule_with_two_subjects_or_none() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let key = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: alice.id.clone(),
            name: "laptop".into(),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    gproxy
        .store()
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("p1".into()),
            name: Set("p1".into()),
            channel: Set("custom".into()),
            config: Set(serde_json::json!({})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();

    let rule = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .permissions()
            .create(PermissionWrite {
                user_id: Some(alice.id.clone()),
                provider_id: Some("p1".into()),
                model_pattern: Some("gpt-*".into()),
                action: "ALLOW".into(),
                priority: Some(10),
                ..PermissionWrite::default()
            })
            .await
            .unwrap()
    });
    // The action is normalized to what the snapshot recognises.
    assert_eq!(rule.action, "allow");
    assert_eq!(rule.model_pattern, "gpt-*");

    // Two subjects.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .permissions()
            .create(PermissionWrite {
                user_id: Some(alice.id.clone()),
                api_key_id: Some(key.key.id.clone()),
                action: "allow".into(),
                ..PermissionWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
    // No subject.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .permissions()
            .create(PermissionWrite {
                action: "allow".into(),
                ..PermissionWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
    // An action the snapshot would drop.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .permissions()
            .create(PermissionWrite {
                user_id: Some(alice.id.clone()),
                action: "maybe".into(),
                ..PermissionWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });
    // A provider that does not exist.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .permissions()
            .create(PermissionWrite {
                user_id: Some(alice.id.clone()),
                provider_id: Some("ghost".into()),
                action: "allow".into(),
                ..PermissionWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 404);
    });
    // Patching one subject onto a rule that already has the other is the same
    // refusal, because the pair is validated as it will end up.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .permissions()
            .update(
                &rule.id,
                gproxy_app::dto::PermissionPatch {
                    api_key_id: Some(Some(key.key.id.clone())),
                    ..gproxy_app::dto::PermissionPatch::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 400);
    });

    // A batch of several rules is one revision, however many rows it names.
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        let written = operations
            .permissions()
            .batch(vec![
                gproxy_app::dto::BatchItem::Create(PermissionWrite {
                    api_key_id: Some(key.key.id.clone()),
                    action: "deny".into(),
                    priority: Some(20),
                    ..PermissionWrite::default()
                }),
                gproxy_app::dto::BatchItem::Delete(rule.id.clone()),
            ])
            .await
            .unwrap();
        assert_eq!(written.len(), 2);
        assert_eq!(written[0].as_ref().unwrap().action, "deny");
        assert!(written[1].is_none());
    });

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    assert_eq!(
        operations
            .permissions()
            .get(&rule.id)
            .await
            .unwrap_err()
            .status_code(),
        404
    );
    assert_eq!(
        operations
            .permissions()
            .list(ListQuery {
                api_key_id: Some(key.key.id.clone()),
                ..ListQuery::default()
            })
            .await
            .unwrap()
            .total,
        1
    );
}

#[tokio::test]
async fn rate_limits_round_trip_and_refuse_an_unusable_window() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();

    let limit = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .rate_limits()
            .create(RateLimitWrite {
                user_id: Some(alice.id.clone()),
                metric: "requests".into(),
                limit_value: "60".into(),
                period_seconds: 60,
                ..RateLimitWrite::default()
            })
            .await
            .unwrap()
    });
    assert_eq!(limit.limit_value, "60");
    assert!(limit.enabled);

    // Zero is a legitimate ceiling: it switches a subject off.
    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        let updated = operations
            .rate_limits()
            .update(
                &limit.id,
                RateLimitPatch {
                    limit_value: Some("0".into()),
                    ..RateLimitPatch::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.limit_value, "0");
    });

    for bad in [
        RateLimitWrite {
            user_id: Some(alice.id.clone()),
            metric: "requests".into(),
            limit_value: "10".into(),
            period_seconds: 0,
            ..RateLimitWrite::default()
        },
        RateLimitWrite {
            user_id: Some(alice.id.clone()),
            metric: "requests".into(),
            limit_value: "-1".into(),
            period_seconds: 60,
            ..RateLimitWrite::default()
        },
        RateLimitWrite {
            user_id: Some(alice.id.clone()),
            metric: "  ".into(),
            limit_value: "10".into(),
            period_seconds: 60,
            ..RateLimitWrite::default()
        },
        RateLimitWrite {
            metric: "requests".into(),
            limit_value: "10".into(),
            period_seconds: 60,
            ..RateLimitWrite::default()
        },
    ] {
        moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
            let error = operations.rate_limits().create(bad).await.unwrap_err();
            assert_eq!(error.status_code(), 400);
        });
    }

    moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.rate_limits().delete(&limit.id).await.unwrap();
    });
}

// ---------------------------------------------------------------------------
// OAuth clients.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn retiring_an_oauth_client_revokes_its_grants_tokens_and_internal_keys() {
    let gproxy = handle().await;
    seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let alice = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();

    let client = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations
            .oauth_clients()
            .create(OAuthClientWrite {
                id: "cli-app".into(),
                name: "CLI".into(),
                redirect_uris: vec!["http://127.0.0.1:1455/callback".into()],
                ..OAuthClientWrite::default()
            })
            .await
            .unwrap()
    });
    assert_eq!(client.redirect_uris.len(), 1);
    assert_eq!(client.deleted_at_ms, None);

    // A relative, fragmented or wildcard redirect is refused.
    for bad in ["/callback", "https://app/cb#frag", "https://*.app/cb"] {
        moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
            let error = operations
                .oauth_clients()
                .create(OAuthClientWrite {
                    id: format!("bad-{}", bad.len()),
                    name: "bad".into(),
                    redirect_uris: vec![bad.into()],
                    ..OAuthClientWrite::default()
                })
                .await
                .unwrap_err();
            assert_eq!(error.status_code(), 400, "{bad}");
        });
    }
    // Registering the same client id twice is a conflict.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .oauth_clients()
            .create(OAuthClientWrite {
                id: "cli-app".into(),
                name: "CLI again".into(),
                ..OAuthClientWrite::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 409);
    });

    // A live grant with an internal key and an issued token, written the way
    // the issuer writes them.
    gproxy
        .store()
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("grant-key".into()),
            user_id: Set(alice.id.clone()),
            name: Set("CLI".into()),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set("11".repeat(32)),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    gproxy
        .store()
        .oauth_grants()
        .create_many(vec![oauth::grant::ActiveModel {
            id: Set("grant-1".into()),
            client_id: Set("cli-app".into()),
            user_id: Set(alice.id.clone()),
            api_key_id: Set("grant-key".into()),
            subject: Set("user".into()),
            scopes: Set(json!([])),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    gproxy
        .store()
        .oauth_tokens()
        .create_many(vec![oauth::token::ActiveModel {
            id: Set("token-1".into()),
            grant_id: Set("grant-1".into()),
            kind: Set(oauth::token::TokenKind::Access),
            token_hash: Set(vec![1; 32]),
            created_at_ms: Set(0),
            expires_at_ms: Set(i64::MAX),
            ..Default::default()
        }])
        .await
        .unwrap();

    let retired = moves_revision!(gproxy, 1, async |operations: Operations<'_, _>| {
        operations.oauth_clients().retire(&client.id).await.unwrap()
    });
    assert!(!retired.enabled);
    assert!(retired.deleted_at_ms.is_some());

    let connection = gproxy.store().connection();
    let grant = oauth::grant::Entity::find_by_id("grant-1".to_string())
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    assert!(
        grant.revoked_at_ms.is_some(),
        "the grant survived retirement"
    );
    let token = oauth::token::Entity::find_by_id("token-1".to_string())
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    assert!(
        token.revoked_at_ms.is_some(),
        "the token survived retirement"
    );
    let key = api_key::Entity::find_by_id("grant-key".to_string())
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    assert!(!key.enabled, "the grant's internal key is still enabled");

    // A retired client cannot be patched back to life, and retiring twice is
    // a conflict rather than a second revision.
    moves_revision!(gproxy, 0, async |operations: Operations<'_, _>| {
        let error = operations
            .oauth_clients()
            .update(
                &client.id,
                gproxy_app::dto::OAuthClientPatch {
                    enabled: Some(true),
                    ..gproxy_app::dto::OAuthClientPatch::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.status_code(), 409);
    });
    // The row is still readable, because session history refers to it.
    let data = snapshot(gproxy.store()).await;
    assert!(
        Operations::new(&gproxy, &data, &config)
            .oauth_clients()
            .get(&client.id)
            .await
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// Audit.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn audit_rows_are_written_outside_the_revision_paged_newest_first_and_redacted() {
    let gproxy = handle().await;
    let root = seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let operations = Operations::new(&gproxy, &data, &config);
    let before = revision(gproxy.store()).await;

    for (index, action) in ["users.create", "users.update", "api_keys.rotate"]
        .into_iter()
        .enumerate()
    {
        operations
            .audit()
            .record_at(
                AuditEntry::new(action)
                    .entity("user", &root)
                    .source_ip(Some("203.0.113.7"))
                    .detail(AuditEntry::redacted(json!({
                        "name": "alice",
                        "password": "hunter2",
                        "nested": { "apiKey": "sk-secret", "keep": 1 },
                    }))),
                1_000 + index as i64,
            )
            .await
            .unwrap();
    }
    // A rejected operation is audited too, with the error's code and not its
    // message.
    operations
        .audit()
        .record_at(
            AuditEntry::new("users.delete")
                .entity("user", &root)
                .detail(AuditEntry::redacted(json!({ "id": root })))
                .failed(&gproxy_app::AppError::Conflict("last admin".into())),
            1_500,
        )
        .await
        .unwrap();

    // The trail is not configuration.
    assert_eq!(revision(gproxy.store()).await, before);

    let page = operations
        .audit()
        .query(AuditQuery {
            page_size: Some(2),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 4);
    assert_eq!(page.items.len(), 2);
    // Newest first.
    assert_eq!(page.items[0].created_at_ms, 1_500);
    assert_eq!(page.items[1].created_at_ms, 1_002);

    // The secrets never reached the column.
    let detail = &page.items[1].detail;
    assert_eq!(detail["name"], json!("alice"));
    assert_eq!(detail["password"], json!(REDACTED));
    assert_eq!(detail["nested"]["apiKey"], json!(REDACTED));
    assert_eq!(detail["nested"]["keep"], json!(1));
    let stored = serde_json::to_string(&page.items).unwrap();
    assert!(!stored.contains("hunter2"), "{stored}");
    assert!(!stored.contains("sk-secret"), "{stored}");

    // Filters: by action substring, by outcome, by time window.
    let family = operations
        .audit()
        .query(AuditQuery {
            action: Some("users.".into()),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(family.total, 3);

    let failed = operations
        .audit()
        .query(AuditQuery {
            outcome: Some("error".into()),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(failed.total, 1);
    assert_eq!(failed.items[0].action, "users.delete");
    assert_eq!(failed.items[0].detail["error"], json!("conflict"));
    assert_eq!(failed.items[0].source_ip, None);

    let window = operations
        .audit()
        .query(AuditQuery {
            since_ms: Some(1_001),
            until_ms: Some(1_500),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(window.total, 2);

    let by_actor = operations
        .audit()
        .query(AuditQuery {
            entity_id: Some(root.clone()),
            ..AuditQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(by_actor.total, 4);
    assert_eq!(by_actor.items[0].source_ip, None);
    assert_eq!(by_actor.items[1].source_ip.as_deref(), Some("203.0.113.7"));
}

#[tokio::test]
async fn an_audit_entry_takes_its_actor_from_the_caller() {
    let gproxy = handle().await;
    let root = seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    let minted = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: root.clone(),
            name: "automation".into(),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let caller = Authenticator::new(gproxy.store(), &data, &config)
        .authenticate_token(&minted.token)
        .await
        .unwrap();

    let operations = Operations::new(&gproxy, &data, &config);
    operations
        .audit()
        .record_at(AuditEntry::new("api_keys.create").by(&caller), 5_000)
        .await
        .unwrap();

    let page = operations
        .audit()
        .query(AuditQuery::default())
        .await
        .unwrap();
    assert_eq!(page.items[0].actor_user_id.as_deref(), Some(root.as_str()));
    assert_eq!(
        page.items[0].actor_api_key_id.as_deref(),
        Some(minted.key.id.as_str())
    );
}

// ---------------------------------------------------------------------------
// The notification a write publishes.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_write_publishes_its_revision_and_scopes_to_the_peers() {
    use gproxy_cache::Cache;

    let cache: Arc<dyn Cache> = Arc::new(MemoryCache::new(MemoryOptions::default()).unwrap());
    let gproxy = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .cache(cache.clone())
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();

    let mut subscription = cache
        .subscribe(gproxy_core::keys::INVALIDATION_TOPIC)
        .await
        .unwrap();

    let data = snapshot(gproxy.store()).await;
    let config = AppConfig::default();
    let created = Operations::new(&gproxy, &data, &config)
        .users()
        .create(UserWrite {
            name: "alice".into(),
            ..UserWrite::default()
        })
        .await
        .unwrap();
    assert_eq!(created.name, "alice");

    let payload = loop {
        // The first notification of any subscription is `ResyncRequired`, so
        // the loop skips everything that is not the message this write sent.
        if let gproxy_cache::Notification::Message(message) = subscription.recv().await.unwrap()
            && let Ok(gproxy_core::Invalidation::ConfigurationChanged { revision, scopes }) =
                serde_json::from_slice::<gproxy_core::Invalidation>(&message)
        {
            break (revision, scopes);
        }
    };
    assert_eq!(
        u64::try_from(revision(gproxy.store()).await).unwrap(),
        payload.0.0
    );
    assert_eq!(payload.1, vec!["identity".to_string()]);
}

// ---------------------------------------------------------------------------
// Nothing in a list leaks a stored credential.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_dto_carries_a_hash_a_sealed_secret_or_a_session_token() {
    let gproxy = handle().await;
    let root = seed_admin(&gproxy).await;
    let config = AppConfig::default();

    let data = snapshot(gproxy.store()).await;
    Operations::new(&gproxy, &data, &config)
        .users()
        .set_password(&root, "correct horse battery staple")
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let minted = Operations::new(&gproxy, &data, &config)
        .api_keys()
        .create(ApiKeyWrite {
            user_id: root.clone(),
            name: "laptop".into(),
            retain_secret: Some(true),
            ..ApiKeyWrite::default()
        })
        .await
        .unwrap();
    let data = snapshot(gproxy.store()).await;
    let session = Authenticator::new(gproxy.store(), &data, &config)
        .create_session(&root, 1_000)
        .await
        .unwrap();

    let operations = Operations::new(&gproxy, &data, &config);
    let users =
        serde_json::to_string(&operations.users().list(ListQuery::default()).await.unwrap())
            .unwrap();
    let keys = serde_json::to_string(
        &operations
            .api_keys()
            .list(ListQuery::default())
            .await
            .unwrap(),
    )
    .unwrap();
    let sessions =
        serde_json::to_string(&operations.sessions().list(&root).await.unwrap()).unwrap();

    // The stored values, read straight from the database, must not appear in
    // any of the three renderings.
    let connection = gproxy.store().connection();
    let stored_user = user::Entity::find_by_id(root.clone())
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    let stored_key = api_key::Entity::find_by_id(minted.key.id.clone())
        .one(connection)
        .await
        .unwrap()
        .unwrap();
    let stored_session = user_session::Entity::find()
        .filter(user_session::Column::UserId.eq(&root))
        .one(connection)
        .await
        .unwrap()
        .unwrap();

    for rendered in [&users, &keys, &sessions] {
        assert!(!rendered.contains(stored_user.password_hash.as_deref().unwrap()));
        assert!(!rendered.contains(&stored_key.key_hash));
        assert!(!rendered.contains(&stored_session.token_hash));
        assert!(!rendered.contains(&minted.token));
        assert!(!rendered.contains(&session.token));
    }
    // What they do carry: the non-secret facts a console renders.
    assert!(users.contains("\"hasPassword\":true"));
    assert!(keys.contains("\"hasSecret\":true"));
    assert!(keys.contains(&minted.key.prefix));
}
