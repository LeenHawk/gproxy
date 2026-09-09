use gproxy_core::{ControlPlane, CoreError, Host, RoutingMode};
use gproxy_store::records::RouteStrategy;
use serde_json::json;
use tokio_rusqlite::rusqlite::Connection;

use crate::{App, MasterKeyConfig, V2ImportOptions};

#[tokio::test]
async fn migrated_model_grants_preserve_scopes_without_exposing_shared_provider_routes() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let key = super::setup::random_key();
    super::setup::v2_database(
        source.path(),
        &key,
        &key,
        &json!({"api_key":super::setup::random_key()}),
        false,
    );
    let connection = Connection::open(source.path().join("gproxy.db")).unwrap();
    connection.execute_batch("INSERT INTO teams VALUES(7,1,'team',1); UPDATE users SET team_id=7;
        INSERT INTO routes VALUES(2,'private-model',1,'weighted'),(3,'slow-model',1,'least_latency');
        INSERT INTO route_members VALUES(2,2,1,'upstream-model',0,100,1),(3,3,1,'upstream-model',0,100,1);
        INSERT INTO route_permissions VALUES(1,'team',7,'public-*',0,0),(2,'org',1,'provider/upstream-model',0,0);").unwrap();
    drop(connection);
    let config = super::test_config(target.path(), MasterKeyConfig::new(None));
    let report = crate::migrate_from_v2(
        &config,
        V2ImportOptions {
            path: source.path().join("gproxy.db"),
            source_master_key: None,
            apply: true,
            merge: false,
        },
    )
    .await
    .unwrap();
    assert!(report.applied && report.issues.is_empty(), "{report}");
    assert!(
        report
            .to_string()
            .contains("scope=team scope_id=7 pattern=\"public-*\"")
    );
    assert!(report.to_string().contains("least_latency"));
    let app = App::start(config).await.unwrap();
    let host = &app.inner.host;
    let control = &host.services.control;
    let request = super::setup::request("migrated-access", "hi", &key);
    let identity = host.authenticate(&request).await.unwrap();
    let snapshot = control.current();
    assert_eq!(
        snapshot
            .routes
            .iter()
            .find(|row| row.name == "private-model")
            .unwrap()
            .strategy,
        RouteStrategy::Weighted
    );
    assert_eq!(
        snapshot
            .routes
            .iter()
            .find(|row| row.name == "slow-model")
            .unwrap()
            .strategy,
        RouteStrategy::RoundRobin
    );
    assert!(
        snapshot
            .permissions
            .iter()
            .any(|row| row.subject_kind == "team" && row.subject_id == identity.team_id.unwrap())
    );
    assert!(
        snapshot.permissions.iter().any(
            |row| row.subject_kind == "organization" && Some(row.subject_id) == identity.org_id
        )
    );
    for (model, mode, allowed) in [
        ("public-model", RoutingMode::Aggregated, true),
        ("private-model", RoutingMode::Aggregated, false),
        ("provider/upstream-model", RoutingMode::Aggregated, true),
        (
            "upstream-model",
            RoutingMode::Scoped {
                provider: "provider".into(),
            },
            true,
        ),
        (
            "public-bypass",
            RoutingMode::Scoped {
                provider: "provider".into(),
            },
            false,
        ),
        (
            "public-model",
            RoutingMode::Named {
                name: "private-model".into(),
            },
            false,
        ),
    ] {
        assert_eq!(
            control.catalogue_visible(&identity, Some(model), &mode),
            allowed,
            "{model} {mode:?}"
        );
        let plan = control.resolve(Some(model), &mode, None).unwrap();
        let mut request = request.clone();
        request.mode = mode;
        let admitted = host
            .admit(
                &identity,
                &request,
                Some(super::generation_operation()),
                Some(model),
                &plan,
            )
            .await;
        if allowed {
            assert!(admitted.is_ok(), "{model}: {admitted:?}");
            host.finish_admission(&request.request_id, None).await;
        } else {
            assert!(
                matches!(admitted, Err(CoreError::Forbidden(_))),
                "{model}: {admitted:?}"
            );
        }
    }
    app.shutdown();
}

#[tokio::test]
async fn runtime_preflight_runs_before_history_reads_for_dry_run_and_apply() {
    let source = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let key = super::setup::random_key();
    super::setup::v2_database(
        source.path(),
        &key,
        &key,
        &json!({"api_key":super::setup::random_key()}),
        true,
    );
    let connection = Connection::open(source.path().join("gproxy.db")).unwrap();
    connection.execute_batch("UPDATE provider_models SET variants_json='42'; UPDATE usages SET metrics_json='not-json';").unwrap();
    drop(connection);
    let config = super::test_config(target.path(), MasterKeyConfig::new(None));
    for apply in [false, true] {
        let report = crate::migrate_from_v2(
            &config,
            V2ImportOptions {
                path: source.path().join("gproxy.db"),
                source_master_key: None,
                apply,
                merge: false,
            },
        )
        .await
        .unwrap();
        assert!(!report.applied && report.has_blockers());
        assert!(report.to_string().contains("model variants"), "{report}");
        assert!(!target.path().join("gproxy.db").exists());
    }
}
