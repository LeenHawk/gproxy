#![cfg(not(target_arch = "wasm32"))]
use gproxy_store::{
    FixedDecimal, Store,
    entity::{
        config::setting,
        limits::quota_window,
        upstream::{provider, provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set},
    },
    operations::{
        quota::{Settlement, SettlementOutcome},
        rewrite::RuleSetReplacement,
        state::{StateChange, StateOutcome, StateValue},
    },
};
use sea_orm::{
    ColumnTrait, ConnectOptions, Database, DatabaseConnection, DbBackend, EntityTrait, QueryFilter,
    Set,
};
use serde_json::json;
async fn database() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    Store::new(db)
}
fn provider(id: &str) -> provider::ActiveModel {
    provider::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        channel: Set("test".into()),
        config: Set(json!({})),
        created_at_ms: Set(0),
        ..Default::default()
    }
}
fn rule(id: &str, set: &str, order: i64) -> rewrite_rule::ActiveModel {
    rewrite_rule::ActiveModel {
        id: Set(id.into()),
        rule_set_id: Set(set.into()),
        pattern: Set("hello".into()),
        replacement: Set("world".into()),
        sort_order: Set(order),
        created_at_ms: Set(0),
        updated_at_ms: Set(0),
        ..Default::default()
    }
}
fn money(s: &str) -> FixedDecimal {
    s.parse().unwrap()
}

#[tokio::test]
async fn batch_crud_conditions_paging_and_singleton_settings() {
    let store = database().await;
    let rows = store
        .providers()
        .create_many(vec![provider("a"), provider("b")])
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].enabled);
    let rows = store
        .providers()
        .get_many(&["b".into(), "missing".into(), "a".into(), "b".into()])
        .await
        .unwrap();
    assert_eq!(rows[0].as_ref().unwrap().id, "b");
    assert!(rows[1].is_none());
    assert_eq!(rows[3], rows[0]);
    let rows = store
        .providers()
        .update_many(vec![
            provider::ActiveModel {
                id: Set("a".into()),
                name: Set("new".into()),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("b".into()),
                enabled: Set(false),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("missing".into()),
                enabled: Set(false),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    assert_eq!(rows[0].as_ref().unwrap().name, "new");
    assert!(!rows[1].as_ref().unwrap().enabled);
    assert!(rows[2].is_none());
    let query = provider::Entity::find().filter(provider::Column::Enabled.eq(true));
    let page = store.providers().page(query, 0, 1).await.unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "a");
    let mut duplicate = provider("c");
    duplicate.name = Set("new".into());
    assert!(
        store
            .providers()
            .create_many(vec![provider("d"), duplicate])
            .await
            .is_err()
    );
    assert!(store.providers().get_many(&["d".into()]).await.unwrap()[0].is_none());
    assert!(store.settings().get().await.unwrap().is_none());
    let settings = store
        .settings()
        .update(setting::ActiveModel {
            max_attempts: Set(3),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(settings.id, 1);
    assert_eq!(settings.max_attempts, 3);
    assert!(
        store
            .settings()
            .update(setting::ActiveModel {
                id: Set(2),
                ..Default::default()
            })
            .await
            .is_err()
    );
    let defaults = store
        .settings()
        .update(setting::ActiveModel::default())
        .await
        .unwrap();
    assert_eq!(defaults.max_attempts, 3);
    assert_eq!(
        store
            .providers()
            .delete_many(&["b".into(), "b".into()])
            .await
            .unwrap(),
        [1, 0]
    );
    let ids = (0..105)
        .map(|i| format!("missing-{i}"))
        .chain(["a".into()])
        .collect::<Vec<_>>();
    let rows = store.providers().get_many(&ids).await.unwrap();
    assert_eq!(rows.len(), ids.len());
    assert!(rows[..105].iter().all(Option::is_none));
    assert_eq!(rows[105].as_ref().unwrap().id, "a");
    assert_eq!(
        store
            .providers()
            .update_where_many(vec![
                provider::Entity::update_many()
                    .col_expr(
                        provider::Column::Enabled,
                        sea_orm::sea_query::Expr::val(false)
                    )
                    .filter(provider::Column::Name.eq("new"))
            ])
            .await
            .unwrap(),
        [1]
    );
    assert_eq!(
        store
            .providers()
            .count_many(vec![
                provider::Entity::find().filter(provider::Column::Enabled.eq(false))
            ])
            .await
            .unwrap(),
        [1]
    );
    assert_eq!(
        store
            .providers()
            .delete_where_many(vec![
                provider::Entity::delete_many().filter(provider::Column::Enabled.eq(false))
            ])
            .await
            .unwrap(),
        [1]
    );
}

#[tokio::test]
async fn rewrite_replacement_order_and_control_snapshot() {
    let store = database().await;
    store
        .providers()
        .create_many(vec![provider("p")])
        .await
        .unwrap();
    store
        .rewrite_rule_sets()
        .create_many(vec![rewrite_rule_set::ActiveModel {
            id: Set("set".into()),
            name: Set("set".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .provider_rewrite_rule_sets()
        .create_many(vec![provider_rewrite_rule_set::ActiveModel {
            id: Set("bind".into()),
            provider_id: Set("p".into()),
            rule_set_id: Set("set".into()),
            sort_order: Set(1),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let result = store
        .rewrite_rule_sets()
        .replace_rules_many(vec![RuleSetReplacement {
            id: "set".into(),
            rules: vec![rule("r1", "set", 10), rule("r2", "set", 2)],
        }])
        .await
        .unwrap();
    assert_eq!(result[0].as_ref().unwrap().rules[0].id, "r2");
    assert!(
        store
            .rewrite_rule_sets()
            .replace_rules_many(vec![RuleSetReplacement {
                id: "set".into(),
                rules: vec![rule("bad", "wrong", 0)]
            }])
            .await
            .is_err()
    );
    let graph = store
        .rewrite_rule_sets()
        .for_providers(&["p".into()])
        .await
        .unwrap();
    assert_eq!(graph[0].rule_sets[0].rules.len(), 2);
    let snapshot = store.load_control_data().await.unwrap();
    assert_eq!(snapshot.providers.len(), 1);
    assert_eq!(snapshot.rewrite_rules[0].id, "r2");
    store
        .rewrite_rule_sets()
        .update_many(vec![rewrite_rule_set::ActiveModel {
            id: Set("set".into()),
            enabled: Set(false),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert!(
        store
            .rewrite_rule_sets()
            .for_providers(&["p".into()])
            .await
            .unwrap()[0]
            .attachments
            .is_empty()
    );
}

#[tokio::test]
async fn exact_idempotent_settlement_and_overflow_rollback() {
    let store = database().await;
    store
        .quota_windows()
        .create_many(vec![quota_window::ActiveModel {
            id: Set("w".into()),
            quota_id: Set("historical".into()),
            starts_at_ms: Set(0),
            used: Set(FixedDecimal::ZERO),
            quota_snapshot: Set(json!({})),
            ..Default::default()
        }])
        .await
        .unwrap();
    let entry = |id: &str, amount: &str| Settlement {
        window_id: "w".into(),
        request_id: id.into(),
        amount: money(amount),
        settled_at_ms: 1,
    };
    assert_eq!(
        store
            .quota_settlements()
            .settle_many(vec![
                entry("a", "0.1"),
                entry("b", "0.2"),
                entry("a", "0.1")
            ])
            .await
            .unwrap(),
        [
            SettlementOutcome::Applied,
            SettlementOutcome::Applied,
            SettlementOutcome::AlreadySettled
        ]
    );
    assert_eq!(
        store.quota_windows().get_many(&["w".into()]).await.unwrap()[0]
            .as_ref()
            .unwrap()
            .used,
        money("0.3")
    );
    assert!(
        store
            .quota_settlements()
            .settle_many(vec![entry("new", "1"), entry("a", "0.4")])
            .await
            .is_err()
    );
    assert!(
        store
            .quota_settlements()
            .get_many(&[("w".into(), "new".into())])
            .await
            .unwrap()[0]
            .is_none()
    );
    store
        .quota_windows()
        .update_many(vec![quota_window::ActiveModel {
            id: Set("w".into()),
            used: Set(FixedDecimal::from_atoms(i64::MAX)),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert!(
        store
            .quota_settlements()
            .settle_many(vec![entry("overflow", "0.000000001")])
            .await
            .is_err()
    );
    assert!(
        store
            .quota_settlements()
            .get_many(&[("w".into(), "overflow".into())])
            .await
            .unwrap()[0]
            .is_none()
    );
}

#[tokio::test]
async fn protocol_state_cas_handles_expiry_and_nonreused_versions() {
    let store = database().await;
    let key = ("scope".to_owned(), "key".to_owned());
    let change = |expected, replacement| StateChange {
        scope: key.0.clone(),
        key: key.1.clone(),
        expected,
        replacement,
    };
    let value = |expires| {
        Some(StateValue {
            payload: b"value".to_vec(),
            expires_at_ms: Some(expires),
        })
    };
    let first = store
        .protocol_states()
        .compare_exchange_many(vec![change(None, value(10))], 0)
        .await
        .unwrap();
    let StateOutcome::Applied(Some(old)) = &first[0] else {
        panic!("not created")
    };
    assert_eq!(
        store
            .protocol_states()
            .compare_exchange_many(vec![change(None, value(10))], 0)
            .await
            .unwrap(),
        [StateOutcome::Conflict]
    );
    assert!(
        store
            .protocol_states()
            .get_live_many(std::slice::from_ref(&key), 10)
            .await
            .unwrap()[0]
            .is_none()
    );
    let fresh = store
        .protocol_states()
        .compare_exchange_many(vec![change(None, value(30))], 10)
        .await
        .unwrap();
    let StateOutcome::Applied(Some(new)) = &fresh[0] else {
        panic!("not recreated")
    };
    assert_ne!(old, new);
    assert_eq!(
        store
            .protocol_states()
            .compare_exchange_many(vec![change(Some(old.clone()), None)], 11)
            .await
            .unwrap(),
        [StateOutcome::Conflict]
    );
    assert_eq!(
        store
            .protocol_states()
            .compare_exchange_many(vec![change(Some(new.clone()), None)], 11)
            .await
            .unwrap(),
        [StateOutcome::Applied(None)]
    );
    assert_eq!(
        store
            .protocol_states()
            .compare_exchange_many(vec![change(None, None)], 11)
            .await
            .unwrap(),
        [StateOutcome::Applied(None)]
    );
}

async fn user_and_credentials(store: &Store<DatabaseConnection>) {
    use gproxy_store::entity::{identity::user, upstream::credential};
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
        .providers()
        .create_many(vec![provider("p")])
        .await
        .unwrap();
    store
        .credentials()
        .create_many(
            ["c1", "c2"]
                .into_iter()
                .map(|id| credential::ActiveModel {
                    id: Set(id.into()),
                    provider_id: Set("p".into()),
                    user_id: Set(Some("u".into())),
                    auth_kind: Set("oauth".into()),
                    secret: Set(b"sealed".to_vec()),
                    metadata: Set(json!({})),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn oauth_exchange_is_single_use_and_revocation_is_live() {
    use gproxy_store::entity::{
        identity::api_key,
        oauth::{client, code, grant},
    };
    use gproxy_store::operations::{
        CasOutcome,
        oauth::{ExchangeSource, IssuedToken, TokenExchange},
    };
    let store = database().await;
    user_and_credentials(&store).await;
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("key".into()),
            user_id: Set("u".into()),
            name: Set("internal".into()),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set("internal-hash".into()),
            prefix: Set("oauth".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_clients()
        .create_many(vec![client::ActiveModel {
            id: Set("client".into()),
            name: Set("client".into()),
            redirect_uris: Set(json!(["http://localhost/callback"])),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_grants()
        .create_many(vec![grant::ActiveModel {
            id: Set("grant".into()),
            user_id: Set("u".into()),
            api_key_id: Set("key".into()),
            client_id: Set("client".into()),
            scopes: Set(json!(["gproxy"])),
            subject: Set("subject".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_codes()
        .create_many(vec![code::ActiveModel {
            id: Set("code".into()),
            code_hash: Set(vec![1; 32]),
            grant_id: Set("grant".into()),
            redirect_uri: Set("http://localhost/callback".into()),
            code_challenge: Set("challenge".into()),
            created_at_ms: Set(0),
            expires_at_ms: Set(1000),
            ..Default::default()
        }])
        .await
        .unwrap();
    let issued = |id: &str, byte, expiry| IssuedToken {
        id: id.into(),
        hash: vec![byte; 32],
        expires_at_ms: expiry,
    };
    let exchange = TokenExchange {
        source: ExchangeSource::Code {
            id: "code".into(),
            hash: vec![1; 32],
            redirect_uri: "http://localhost/callback".into(),
            code_challenge: "challenge".into(),
        },
        grant_id: "grant".into(),
        client_id: "client".into(),
        access: issued("a1", 2, 500),
        refresh: issued("r1", 3, 10000),
        now_ms: 100,
    };
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![exchange.clone()])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![exchange])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert!(
        store
            .oauth_grants()
            .resolve_access_many(&[vec![2; 32]], 101)
            .await
            .unwrap()[0]
            .is_some()
    );
    let refresh = TokenExchange {
        source: ExchangeSource::Refresh {
            id: "r1".into(),
            hash: vec![3; 32],
        },
        grant_id: "grant".into(),
        client_id: "client".into(),
        access: issued("a2", 4, 1000),
        refresh: issued("r2", 5, 20000),
        now_ms: 200,
    };
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![refresh.clone()])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert_eq!(
        store
            .oauth_grants()
            .exchange_tokens_many(vec![refresh])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    let grant = store
        .oauth_grants()
        .get_many(&["grant".into()])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(grant.refresh_count, 1);
    assert_eq!(grant.logged_in_at_ms, Some(100));
    assert_eq!(
        store
            .oauth_grants()
            .revoke_many(&["grant".into()], 300)
            .await
            .unwrap(),
        [true]
    );
    assert!(
        store
            .oauth_grants()
            .resolve_access_many(&[vec![4; 32]], 301)
            .await
            .unwrap()[0]
            .is_none()
    );
}
#[tokio::test]
async fn agent_handoff_is_fenced_and_preserves_old_targets() {
    use gproxy_store::entity::resource::{
        agent_assignment as assignment, agent_session as session, resource_binding,
    };
    use gproxy_store::operations::{
        CasOutcome,
        agents::{AssignmentActivation, AssignmentReservation},
    };
    let store = database().await;
    user_and_credentials(&store).await;
    store
        .agent_sessions()
        .create_many(vec![session::ActiveModel {
            id: Set("s".into()),
            user_id: Set("u".into()),
            scope: Set("agent".into()),
            affinity_key: Set("client".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let reserve = |id: &str, version, previous, credential: &str, reason| AssignmentReservation {
        id: id.into(),
        session_id: "s".into(),
        expected_version: version,
        previous_generation: previous,
        provider_id: "p".into(),
        credential_id: credential.into(),
        reason,
        exhausted_cycle_id: None,
        trigger_request_id: Some("request".into()),
        now_ms: 1,
    };
    let activate = |id: &str, version, generation| AssignmentActivation {
        session_id: "s".into(),
        assignment_id: id.into(),
        expected_version: version,
        generation,
        now_ms: 2,
    };
    assert_eq!(
        store
            .agent_sessions()
            .reserve_assignments_many(vec![reserve(
                "a",
                0,
                None,
                "c1",
                assignment::AssignmentReason::Initial
            )])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert_eq!(
        store
            .agent_sessions()
            .activate_assignments_many(vec![activate("a", 1, 1)])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    store
        .resource_bindings()
        .create_many(vec![resource_binding::ActiveModel {
            id: Set("resource".into()),
            scope: Set("u/agent/s".into()),
            kind: Set("server".into()),
            public_id: Set("public".into()),
            generation: Set(1),
            assignment_id: Set(Some("a".into())),
            provider_id: Set("p".into()),
            upstream_id: Set(Some("upstream-a".into())),
            user_id: Set(Some("u".into())),
            credential_id: Set("c1".into()),
            created_at_ms: Set(1),
            updated_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store
            .agent_sessions()
            .reserve_assignments_many(vec![reserve(
                "b",
                2,
                Some(1),
                "c2",
                assignment::AssignmentReason::QuotaExhausted
            )])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert!(
        store
            .agent_sessions()
            .current_assignments_many(&["s".into()], 2)
            .await
            .unwrap()[0]
            .is_none()
    );
    assert_eq!(
        store
            .agent_sessions()
            .activate_assignments_many(vec![activate("a", 1, 1)])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert_eq!(
        store
            .agent_sessions()
            .activate_assignments_many(vec![activate("b", 3, 3)])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert_eq!(
        store
            .agent_sessions()
            .current_assignments_many(&["s".into()], 3)
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .credential_id,
        "c2"
    );
    assert_eq!(
        store
            .agent_assignments()
            .get_many(&["a".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .state,
        assignment::AssignmentState::Replaced
    );
    store
        .credentials()
        .delete_many(&["c1".into()])
        .await
        .unwrap();
    assert_eq!(
        store
            .resource_bindings()
            .get_many(&["resource".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .credential_id,
        "c1"
    );
}

#[tokio::test]
async fn authorization_device_approval_and_client_retirement_are_atomic() {
    use gproxy_store::entity::{
        identity::api_key,
        oauth::{client, code, device, grant},
    };
    use gproxy_store::operations::{
        CasOutcome,
        oauth::{Authorization, DeviceApproval},
    };
    let store = database().await;
    user_and_credentials(&store).await;
    store
        .oauth_clients()
        .create_many(vec![client::ActiveModel {
            id: Set("client".into()),
            name: Set("client".into()),
            redirect_uris: Set(json!([])),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .oauth_devices()
        .create_many(vec![device::ActiveModel {
            id: Set("device".into()),
            device_code_hash: Set(vec![8; 32]),
            user_code: Set("CODE".into()),
            client_id: Set("client".into()),
            scopes: Set(json!(["gproxy"])),
            created_at_ms: Set(0),
            expires_at_ms: Set(1000),
            ..Default::default()
        }])
        .await
        .unwrap();
    let issue = |suffix: &str, hash| Authorization {
        api_key: api_key::ActiveModel {
            id: Set(format!("k-{suffix}")),
            user_id: Set("u".into()),
            name: Set("oauth".into()),
            kind: Set(api_key::ApiKeyKind::OAuth),
            key_hash: Set(format!("hash-{suffix}")),
            prefix: Set("oauth".into()),
            ..Default::default()
        },
        grant: grant::ActiveModel {
            id: Set(format!("g-{suffix}")),
            user_id: Set("u".into()),
            api_key_id: Set(format!("k-{suffix}")),
            client_id: Set("client".into()),
            scopes: Set(json!(["gproxy"])),
            subject: Set("u".into()),
            created_at_ms: Set(1),
            ..Default::default()
        },
        code: code::ActiveModel {
            id: Set(format!("c-{suffix}")),
            code_hash: Set(vec![hash; 32]),
            grant_id: Set(format!("g-{suffix}")),
            redirect_uri: Set("http://localhost/cb".into()),
            code_challenge: Set("challenge".into()),
            created_at_ms: Set(1),
            expires_at_ms: Set(1000),
            ..Default::default()
        },
        device: Some(DeviceApproval {
            id: "device".into(),
            authorization_payload: Some(b"sealed-code-verifier".to_vec()),
        }),
        now_ms: 1,
    };
    assert_eq!(
        store
            .oauth_grants()
            .issue_many(vec![issue("first", 1)])
            .await
            .unwrap(),
        [CasOutcome::Applied]
    );
    assert_eq!(
        store
            .oauth_grants()
            .issue_many(vec![issue("second", 2)])
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    assert!(
        store
            .api_keys()
            .get_many(&["k-second".into()])
            .await
            .unwrap()[0]
            .is_none()
    );
    let device = store
        .oauth_devices()
        .get_many(&["device".into()])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(device.grant_id.as_deref(), Some("g-first"));
    assert_eq!(
        store
            .oauth_devices()
            .deny_pending_many(&["device".into()], 3)
            .await
            .unwrap(),
        [CasOutcome::Conflict]
    );
    let mut conflicting = issue("third", 1);
    conflicting.device = None;
    assert!(
        store
            .oauth_grants()
            .issue_many(vec![conflicting])
            .await
            .is_err()
    );
    assert!(
        store
            .api_keys()
            .get_many(&["k-third".into()])
            .await
            .unwrap()[0]
            .is_none()
    );
    store
        .oauth_clients()
        .retire_many(&["client".into()], 5)
        .await
        .unwrap();
    assert_eq!(
        store
            .oauth_grants()
            .get_many(&["g-first".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .revoked_at_ms,
        Some(5)
    );
    store
        .oauth_clients()
        .update_many(vec![client::ActiveModel {
            id: Set("client".into()),
            enabled: Set(true),
            deleted_at_ms: Set(None),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert!(
        store
            .oauth_grants()
            .get_many(&["g-first".into()])
            .await
            .unwrap()[0]
            .as_ref()
            .unwrap()
            .revoked_at_ms
            .is_some()
    );
}

#[tokio::test]
async fn concurrent_settlement_and_credential_cas_have_one_winner() {
    use gproxy_store::operations::{CasOutcome, credentials::CredentialRefresh};
    let store = database().await;
    user_and_credentials(&store).await;
    store
        .quota_windows()
        .create_many(vec![quota_window::ActiveModel {
            id: Set("w".into()),
            quota_id: Set("history".into()),
            starts_at_ms: Set(0),
            used: Set(FixedDecimal::ZERO),
            quota_snapshot: Set(json!({})),
            ..Default::default()
        }])
        .await
        .unwrap();
    let first = store.quota_settlements();
    let second = store.quota_settlements();
    let entry = Settlement {
        window_id: "w".into(),
        request_id: "same".into(),
        amount: money("9007199.254740993"),
        settled_at_ms: 1,
    };
    let (a, b) = tokio::join!(
        first.settle_many(vec![entry.clone()]),
        second.settle_many(vec![entry.clone()])
    );
    let outcomes = [a.unwrap()[0], b.unwrap()[0]];
    assert_eq!(
        outcomes
            .iter()
            .filter(|o| **o == SettlementOutcome::Applied)
            .count(),
        1
    );
    assert_eq!(
        store.quota_windows().get_many(&["w".into()]).await.unwrap()[0]
            .as_ref()
            .unwrap()
            .used,
        entry.amount
    );
    let refresh = CredentialRefresh {
        id: "c1".into(),
        expected_version: 0,
        secret: b"new".to_vec(),
        expires_at_ms: Some(100),
    };
    let one = store.credentials();
    let two = store.credentials();
    let (a, b) = tokio::join!(
        one.refresh_many(vec![refresh.clone()]),
        two.refresh_many(vec![refresh])
    );
    assert_eq!(
        [a.unwrap()[0], b.unwrap()[0]]
            .iter()
            .filter(|o| **o == CasOutcome::Applied)
            .count(),
        1
    );
}
