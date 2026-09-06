use std::collections::HashMap;
use std::ffi::OsString;

use super::*;

/// Mirrors what clap's `env` does, so the test exercises the same
/// layering without depending on the real process environment.
fn from_environment(environment: &HashMap<&str, OsString>) -> Cli {
    let get = |name: &str| {
        environment
            .get(name)
            .and_then(|value| value.clone().into_string().ok())
    };
    Cli {
        update_channel: get("GPROXY_UPDATE_CHANNEL_SERVE"),
        restart_parent: None,
        command: None,
        host: get(HOST),
        port: get(PORT),
        data_dir: get(DATA_DIR),
        persistence: get(PERSISTENCE),
        dsn: get(DSN),
        libsql_url: get(LIBSQL_URL),
        libsql_auth_token: get(LIBSQL_AUTH_TOKEN),
        redis_url: get(REDIS_URL),
        upstash_url: get(UPSTASH_URL),
        upstash_token: get(UPSTASH_TOKEN),
        master_key: get(MASTER_KEY),
        master_key_next: get(MASTER_KEY_NEXT),
        master_key_rotate: get(MASTER_KEY_ROTATE),
        upstream_proxy_url: get(UPSTREAM_PROXY_URL),
        instance_id: get(INSTANCE_ID),
        max_attempts: get(MAX_ATTEMPTS),
        max_in_flight: get(MAX_IN_FLIGHT),
        file_upload_max_in_flight: get(FILE_UPLOAD_MAX_IN_FLIGHT),
        trusted_proxies: get(TRUSTED_PROXIES),
        cors_origins: get(CORS_ORIGINS),
        log_format: get(LOG_FORMAT),
        admin_user: get(ADMIN_USER),
        admin_password: get(ADMIN_PASSWORD),
        bootstrap_admin_api_key: get(BOOTSTRAP_ADMIN_API_KEY),
        bootstrap_channels: get(BOOTSTRAP_CHANNELS),
    }
}

#[test]
fn dotenv_parsing_uses_real_environment_first() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(".env"),
        "# comment\nGPROXY_PORT=1000 # inline\nGPROXY_DATA_DIR=data-home\nDENO_DEPLOY_TOKEN=not-ours\n",
    )
    .unwrap();
    std::fs::create_dir(directory.path().join("data-home")).unwrap();
    std::fs::write(
        directory.path().join("data-home/.env"),
        "GPROXY_PORT=2000\nGPROXY_PERSISTENCE=sqlite\n",
    )
    .unwrap();
    let environment = HashMap::from([(PORT, OsString::from("3000"))]);
    let NativeCommand::Serve(config) =
        resolve(from_environment(&environment), directory.path()).unwrap()
    else {
        panic!("expected serve command");
    };
    assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
    assert_eq!(config.data_dir(), directory.path().join("data-home"));
    assert!(
        !read_dotenv(&directory.path().join(".env"))
            .unwrap()
            .contains_key("DENO_DEPLOY_TOKEN")
    );
}

#[test]
fn command_line_wins_over_environment_and_dotenv() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join(".env"), "GPROXY_PORT=1000\n").unwrap();
    let mut cli = from_environment(&HashMap::from([(PORT, OsString::from("2000"))]));
    cli.port = Some("3000".into());
    let NativeCommand::Serve(config) = resolve(cli, directory.path()).unwrap() else {
        panic!("expected serve command");
    };
    assert_eq!(config.listen_addr().to_string(), "127.0.0.1:3000");
}

#[test]
fn v2_native_flags_keep_the_database_path_and_update_channel() {
    use clap::Parser as _;
    let directory = tempfile::tempdir().unwrap();
    let cli = Cli::try_parse_from([
        "gproxy",
        "--persistence",
        "db",
        "--dsn",
        "sqlite://legacy/gproxy.db?mode=rwc",
        "--update-channel",
        "staging",
        "--gproxy-restart-parent",
        "123",
    ])
    .unwrap();
    let NativeCommand::Serve(config) = resolve(cli, directory.path()).unwrap() else {
        panic!("expected serve");
    };
    let gproxy_store::BackendConfig::Sqlite { path } = config.backend_config() else {
        panic!("expected SQLite");
    };
    assert_eq!(path, directory.path().join("legacy/gproxy.db"));
    assert_eq!(config.update_channel(), Some("staging"));
    assert_eq!(config.restart_parent(), Some(123));
    assert!(
        super::persistence::sqlite(
            config,
            Some("postgres://remote/db".into()),
            directory.path()
        )
        .is_err()
    );
}
