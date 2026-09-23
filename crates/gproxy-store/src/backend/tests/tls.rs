use super::super::{Executor, Statement, mysql::Mysql, postgres::Postgres};

#[tokio::test]
async fn postgres_required_tls_rejects_a_plaintext_server() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8];
        socket.read_exact(&mut request).await.unwrap();
        assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
        socket.write_all(b"N").await.unwrap();
        let mut extra = [0];
        assert_eq!(socket.read(&mut extra).await.unwrap(), 0);
    });
    let error = Postgres::connect(&format!(
        "postgres://user:secret@{address}/test?sslmode=require&connect_timeout=5"
    ))
    .await
    .err()
    .expect("TLS is required");
    let message = error.to_string();
    assert!(message.contains("TLS"), "{message}");
    assert!(!message.contains("secret"));
    server.await.unwrap();
}

#[test]
fn mysql_tls_checks_certificates_and_reports_invalid_options_without_values() {
    let options =
        mysql_async::Opts::from_url("mysql://user:secret@localhost/test?require_ssl=true").unwrap();
    let tls = options.ssl_opts().expect("TLS enabled");
    assert!(!tls.accept_invalid_certs());
    assert!(!tls.skip_domain_validation());
    let error = Mysql::connect("mysql://user:secret@localhost/test?require_ssl=private-value")
        .err()
        .expect("invalid TLS setting");
    let message = error.to_string();
    assert!(message.contains("require_ssl"));
    assert!(!message.contains("secret"));
    assert!(!message.contains("private-value"));
}

#[tokio::test]
#[ignore = "requires GPROXY_TEST_POSTGRES_TLS_DSN; only performs a read-only TLS query"]
async fn postgres_live_tls_connection() {
    let dsn = std::env::var("GPROXY_TEST_POSTGRES_TLS_DSN").unwrap();
    assert_eq!(
        dsn.parse::<tokio_postgres::Config>()
            .unwrap()
            .get_ssl_mode(),
        tokio_postgres::config::SslMode::Require,
    );
    let database = Postgres::connect(&dsn).await.unwrap();
    // Hosted proxies may terminate TLS before the server, so pg_stat_ssl does
    // not describe the client connection. Require TLS in the connector above.
    let result = database
        .execute(Statement::plain("SELECT 1::bigint AS connected"))
        .await
        .unwrap();
    assert_eq!(result.rows[0].i64("connected").unwrap(), 1);
}

#[tokio::test]
#[ignore = "requires GPROXY_TEST_MYSQL_TLS_DSN; only performs a read-only TLS query"]
async fn mysql_live_tls_connection() {
    let dsn = std::env::var("GPROXY_TEST_MYSQL_TLS_DSN").unwrap();
    let database = Mysql::connect(&dsn).unwrap();
    let result = database
        .execute(Statement::plain("SHOW SESSION STATUS LIKE 'Ssl_cipher'"))
        .await
        .unwrap();
    assert!(!result.rows[0].text("Value").unwrap().is_empty());
}
