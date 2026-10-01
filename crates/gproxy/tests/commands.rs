//! `migrate`, `bootstrap admin` and a real `serve` — the first end-to-end proof
//! that v4 runs as a process.
//!
//! Every test here works against a fresh temporary data directory and a SQLite
//! file inside it, which is the deployment shape the default configuration
//! describes. Nothing reaches a network except the one test that binds an
//! ephemeral port on loopback and talks to itself.
//!
//! No test in this file reads the environment, so none of them needs the lock
//! `tests/config.rs` holds.

use gproxy::{
    bootstrap,
    config::{AdminOptions, Settings, TelemetryOptions},
    instance,
};
use gproxy_app::{AppConfig, config::StoreBackendConfig};

/// A settings value for a throwaway instance in `directory`.
fn settings(directory: &std::path::Path, admin: AdminOptions) -> Settings {
    Settings {
        config: AppConfig {
            // Port 0: the kernel picks one, so two tests never collide.
            port: 0,
            data_dir: Some(directory.to_string_lossy().into_owned()),
            store: StoreBackendConfig::Sqlite {
                path: "gproxy.db".into(),
            },
            // Nothing is embedded in a test binary, and a test must not depend
            // on whether a console happens to have been built.
            console: gproxy_app::config::ConsoleConfig {
                enabled: false,
                path: None,
            },
            ..AppConfig::default()
        },
        admin,
        telemetry: TelemetryOptions::default(),
        instance_id: None,
    }
}

async fn open(settings: &Settings) -> instance::Instance {
    instance::open(settings, instance::OpenOptions::management())
        .await
        .expect("open the instance")
}

// -------------------------------------------------------------- migrate --

#[tokio::test]
async fn migrate_creates_the_schema_on_a_fresh_file_and_succeeds() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());

    instance::migrate(&settings.config, false)
        .await
        .expect("migrate a fresh database");
    assert!(directory.path().join("gproxy.db").is_file());

    // Idempotent: running it again on the database it just made changes nothing
    // and still succeeds, which is what a deployment's migration step needs.
    instance::migrate(&settings.config, false)
        .await
        .expect("migrate an up-to-date database");

    // And the settings row is there, so `serve` needs no DDL of its own.
    let store = gproxy_store::Store::new(instance::connect(&settings.config).await.unwrap());
    assert!(store.settings().get().await.unwrap().is_some());
}

#[tokio::test]
async fn migrate_creates_the_data_directory_it_was_given() {
    let parent = tempfile::tempdir().unwrap();
    let nested = parent.path().join("a/b/c");
    let settings = settings(&nested, AdminOptions::default());

    instance::migrate(&settings.config, false).await.unwrap();
    assert!(nested.join("gproxy.db").is_file());
}

/// `--status` answers and stops. Opening a SQLite file creates it — that is
/// what `mode=rwc` means — but nothing inside it, not even the ledger the
/// question is about.
#[tokio::test]
async fn migrate_status_reports_without_changing_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());

    instance::migrate(&settings.config, true)
        .await
        .expect("report on an empty database");
    assert!(tables(&settings).await.is_empty());

    instance::migrate(&settings.config, false).await.unwrap();
    let after = tables(&settings).await;
    instance::migrate(&settings.config, true).await.unwrap();
    assert_eq!(tables(&settings).await, after);
    assert!(after.contains(&gproxy_store::MIGRATION_LEDGER.to_owned()));
}

/// The case the migrator was written for. The message has to be one an operator
/// can act on, and the database has to come out of it exactly as it went in.
#[tokio::test]
async fn migrate_refuses_a_database_this_build_did_not_create() {
    use sea_orm::ConnectionTrait;
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let connection = instance::connect(&settings.config).await.unwrap();
    for statement in [
        "CREATE TABLE unrelated_migrations (version varchar NOT NULL PRIMARY KEY)",
        "CREATE TABLE users (id integer NOT NULL PRIMARY KEY, name varchar)",
    ] {
        connection.execute_unprepared(statement).await.unwrap();
    }
    drop(connection);

    let error = instance::migrate(&settings.config, false)
        .await
        .expect_err("a database with a foreign schema")
        .to_string();
    assert!(error.contains("neither GPROXY"), "{error}");
    assert!(error.contains("schema_migrations"), "{error}");
    assert_eq!(tables(&settings).await, ["unrelated_migrations", "users"]);
}

/// The tables in the instance's database, sorted, engine-owned ones excluded.
async fn tables(settings: &Settings) -> Vec<String> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let connection = sea_orm::Database::connect(format!(
        "sqlite://{}?mode=ro",
        std::path::Path::new(settings.config.data_dir.as_deref().unwrap())
            .join("gproxy.db")
            .display()
    ))
    .await
    .unwrap();
    let mut names: Vec<String> = connection
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.try_get("", "name").unwrap())
        .collect();
    names.sort();
    names
}

// ------------------------------------------------------------ bootstrap --

#[tokio::test]
async fn bootstrap_creates_exactly_one_admin_and_is_a_no_op_the_second_time() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let instance = open(&settings).await;

    let first = bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .expect("bootstrap a fresh instance");
    assert!(first.created(), "{first:?}");
    let bootstrap::Report::Created {
        user,
        password,
        api_key,
    } = &first
    else {
        panic!("not created: {first:?}");
    };
    assert_eq!(user, "admin");
    // Neither was supplied, so both were generated and both must be reported —
    // this is the only time they will ever be printable.
    assert!(password.is_some());
    assert!(api_key.is_some());

    let rows = users(&instance).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].role, "admin");
    assert!(rows[0].enabled);
    assert!(rows[0].password_hash.is_some());
    assert_eq!(keys(&instance).await.len(), 1);

    // Second run: nothing written, nothing printed.
    let second = bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .expect("bootstrap an instance that is already set up");
    assert!(matches!(
        second,
        bootstrap::Report::AlreadySetUp { users: 1 }
    ));
    assert_eq!(users(&instance).await.len(), 1);
    assert_eq!(keys(&instance).await.len(), 1);

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn an_explicit_admin_password_overrides_on_restart_without_replacing_keys() {
    let directory = tempfile::tempdir().unwrap();
    let first = settings(directory.path(), AdminOptions::default());
    let instance = open(&first).await;
    bootstrap::ensure_admin(&instance.app, &first.admin)
        .await
        .unwrap();
    let before = users(&instance).await;

    // A supplied password is authoritative; the bootstrap key is still first-run only.
    let again = settings(
        directory.path(),
        AdminOptions {
            user: "admin".into(),
            password: Some("a-different-password".into()),
            api_key: Some("sk-a-different-key".into()),
        },
    );
    let report = bootstrap::ensure_admin(&instance.app, &again.admin)
        .await
        .unwrap();
    assert!(!report.created());
    assert!(matches!(report, bootstrap::Report::PasswordUpdated { .. }));

    let after = users(&instance).await;
    assert_eq!(after.len(), 1);
    assert_ne!(after[0].password_hash, before[0].password_hash);
    assert!(gproxy_app::auth::password::verify(
        "a-different-password",
        after[0].password_hash.as_deref().unwrap()
    ));
    assert_eq!(keys(&instance).await.len(), 1);

    let repeated = bootstrap::ensure_admin(&instance.app, &again.admin)
        .await
        .unwrap();
    assert!(matches!(repeated, bootstrap::Report::AlreadySetUp { .. }));
    assert_eq!(
        users(&instance).await[0].password_hash,
        after[0].password_hash
    );

    instance.app.gproxy().shutdown();
}

#[tokio::test]
async fn a_supplied_password_and_key_are_used_and_not_echoed_back() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(
        directory.path(),
        AdminOptions {
            user: "operator".into(),
            password: Some("a-supplied-password".into()),
            api_key: Some("sk-a-supplied-key".into()),
        },
    );
    let instance = open(&settings).await;

    let report = bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .unwrap();
    let bootstrap::Report::Created {
        user,
        password,
        api_key,
    } = &report
    else {
        panic!("not created: {report:?}");
    };
    assert_eq!(user, "operator");
    // The operator has both already; repeating them would only put them in a
    // terminal scrollback for no reason.
    assert_eq!(password.as_deref(), None);
    assert_eq!(api_key.as_deref(), None);

    instance.app.gproxy().shutdown();
}

/// The v3 regression this phase had to avoid: a bootstrap API key with an `sk-`
/// prefix was stored under a digest the admin API did not look it up by, so every
/// request with it answered 401.
#[tokio::test]
async fn the_minted_bootstrap_api_key_authenticates() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let instance = open(&settings).await;

    let report = bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .unwrap();
    let bootstrap::Report::Created {
        api_key: Some(token),
        ..
    } = report
    else {
        panic!("no key was minted");
    };
    assert!(token.starts_with("sk-"), "{token}");

    let data = instance.app.data();
    let caller = instance
        .app
        .authenticator(&data)
        .authenticate_token(&token)
        .await
        .expect("the minted key authenticates");
    assert_eq!(caller.user_role, "admin");
    assert!(caller.api_key_id.is_some());

    // And a key that was never minted does not.
    assert!(
        instance
            .app
            .authenticator(&data)
            .authenticate_token("sk-not-a-key")
            .await
            .is_err()
    );

    instance.app.gproxy().shutdown();
}

/// The same assertion for a key the operator supplied, which is the shape the v3
/// bug actually took: an operator-chosen `sk-…` value.
#[tokio::test]
async fn an_operator_supplied_bootstrap_api_key_authenticates() {
    let directory = tempfile::tempdir().unwrap();
    for token in [
        "sk-operator-chosen-key",
        "at-operator-chosen-key",
        "no-prefix-key",
    ] {
        let directory = directory.path().join(token.replace(['-'], "_"));
        let settings = settings(
            &directory,
            AdminOptions {
                user: "admin".into(),
                password: Some("a-supplied-password".into()),
                api_key: Some(token.to_owned()),
            },
        );
        let instance = open(&settings).await;
        bootstrap::ensure_admin(&instance.app, &settings.admin)
            .await
            .unwrap();

        let data = instance.app.data();
        let caller = instance
            .app
            .authenticator(&data)
            .authenticate_token(token)
            .await
            .unwrap_or_else(|error| panic!("`{token}` did not authenticate: {error}"));
        assert_eq!(caller.user_role, "admin");
        instance.app.gproxy().shutdown();
    }
}

// ---------------------------------------------------------------- serve --

/// The end-to-end proof: bind an ephemeral port, answer `/healthz`, shut down.
///
/// This drives the same [`gproxy_host_axum::router`] over the same
/// [`gproxy_app::App`] that `serve` does, differing only in that the shutdown
/// signal is a channel rather than `SIGTERM` — a test cannot send itself a
/// signal without racing the harness.
#[tokio::test]
async fn the_server_binds_answers_healthz_and_shuts_down() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let instance = instance::open(&settings, instance::OpenOptions::serving())
        .await
        .unwrap();
    bootstrap::ensure_admin(&instance.app, &settings.admin)
        .await
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, shutdown) = tokio::sync::oneshot::channel::<()>();

    let state = gproxy_host_axum::HostState::new(instance.app.clone());
    let service = gproxy_host_axum::router(state)
        .into_make_service_with_connect_info::<std::net::SocketAddr>();
    let server = tokio::spawn(async move {
        axum::serve(listener, service)
            .with_graceful_shutdown(async move {
                let _ = shutdown.await;
            })
            .await
    });

    let body = get(address, "/healthz").await;
    let health: serde_json::Value = serde_json::from_str(&body).expect("healthz is json");
    assert_eq!(health["status"], "ok");
    assert!(health["revision"].as_i64().is_some(), "{health}");

    // The console is switched off in these settings, so it 404s rather than
    // pretending to serve an application that is not there.
    assert!(get(address, "/console").await.contains("not enabled"));

    stop.send(()).ok();
    server.await.unwrap().unwrap();
    instance.app.gproxy().shutdown();
}

/// One GET over a raw socket. A full HTTP client is a dependency this crate does
/// not otherwise need, and `/healthz` answers a short body with a known shape.
async fn get(address: std::net::SocketAddr, path: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no headers in {response}"));
    assert!(
        head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.1 404"),
        "{head}"
    );
    body.to_owned()
}

// ------------------------------------------------------------- transfer --

/// One configuration moved from one instance to another, which is what `export`
/// and `import` exist for.
#[tokio::test]
async fn a_configuration_round_trips_between_two_instances() {
    let source_dir = tempfile::tempdir().unwrap();
    let source_settings = settings(source_dir.path(), AdminOptions::default());
    let source = open(&source_settings).await;
    source
        .app
        .gproxy()
        .manage()
        .providers()
        .create(gproxy_sdk::dto::ProviderWrite {
            name: "upstream".into(),
            channel: "custom".into(),
            base_url: Some("https://example.invalid".into()),
            ..Default::default()
        })
        .await
        .expect("create a provider");

    let document = source_dir.path().join("export.json");
    gproxy::transfer::export(&source.app, &document, false)
        .await
        .expect("export");
    assert!(document.is_file());
    source.app.gproxy().shutdown();
    drop(source);

    // A second, empty instance takes the document.
    let destination_dir = tempfile::tempdir().unwrap();
    let destination_settings = settings(destination_dir.path(), AdminOptions::default());
    let destination = open(&destination_settings).await;
    assert!(providers(&destination).await.is_empty());

    gproxy::transfer::import(
        &destination.app,
        &document,
        gproxy::cli::ImportModeArg::Merge,
        None,
    )
    .await
    .expect("import");

    let arrived = providers(&destination).await;
    assert_eq!(arrived.len(), 1, "{arrived:?}");
    assert_eq!(arrived[0].name, "upstream");
    assert_eq!(arrived[0].channel, "custom");

    // Replaying the same document is not a second provider: ids travel with it.
    gproxy::transfer::import(
        &destination.app,
        &document,
        gproxy::cli::ImportModeArg::Merge,
        None,
    )
    .await
    .expect("re-import");
    assert_eq!(providers(&destination).await.len(), 1);

    destination.app.gproxy().shutdown();
}

#[tokio::test]
async fn importing_something_that_is_not_an_export_says_so() {
    let directory = tempfile::tempdir().unwrap();
    let settings = settings(directory.path(), AdminOptions::default());
    let instance = open(&settings).await;

    let junk = directory.path().join("junk.json");
    std::fs::write(&junk, b"{\"hello\": true}").unwrap();
    let error = gproxy::transfer::import(
        &instance.app,
        &junk,
        gproxy::cli::ImportModeArg::Merge,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("not a gproxy configuration export"),
        "{error}"
    );

    instance.app.gproxy().shutdown();
}

// --------------------------------------------------------------- helpers --

async fn users(instance: &instance::Instance) -> Vec<gproxy_store::entity::identity::user::Model> {
    use sea_orm::EntityTrait;
    instance
        .app
        .gproxy()
        .store()
        .users()
        .query(gproxy_store::entity::identity::user::Entity::find())
        .await
        .unwrap()
}

async fn keys(
    instance: &instance::Instance,
) -> Vec<gproxy_store::entity::identity::api_key::Model> {
    use sea_orm::EntityTrait;
    instance
        .app
        .gproxy()
        .store()
        .api_keys()
        .query(gproxy_store::entity::identity::api_key::Entity::find())
        .await
        .unwrap()
}

async fn providers(
    instance: &instance::Instance,
) -> Vec<gproxy_store::entity::upstream::provider::Model> {
    use sea_orm::EntityTrait;
    instance
        .app
        .gproxy()
        .store()
        .providers()
        .query(gproxy_store::entity::upstream::provider::Entity::find())
        .await
        .unwrap()
}
