use super::*;

#[test]
fn secret_cli_password_is_not_persisted() {
    let args = [
        "--host",
        "127.0.0.1",
        "--admin-password=secret",
        "--port",
        "8787",
    ]
    .into_iter()
    .map(OsString::from);
    assert_eq!(
        safe_serve_args(args),
        ["--host", "127.0.0.1", "--port", "8787"].map(OsString::from)
    );
}

#[test]
fn split_password_value_is_removed() {
    let args = ["--admin-password", "secret", "--port", "8787"]
        .into_iter()
        .map(OsString::from);
    assert_eq!(
        safe_serve_args(args),
        ["--port", "8787"].map(OsString::from)
    );
}

#[test]
fn connection_urls_are_not_persisted() {
    let args = [
        "--dsn=postgres://user:secret@db/gproxy",
        "--redis-url",
        "redis://:secret@cache",
        "--port",
        "8787",
    ]
    .into_iter()
    .map(OsString::from);
    assert_eq!(
        safe_serve_args(args),
        ["--port", "8787"].map(OsString::from)
    );
}
