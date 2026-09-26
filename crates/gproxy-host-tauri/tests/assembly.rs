//! The whole shape, started for real against a temporary data directory.
//!
//! No window and no display server: [`Desktop::start`] is the library, and the
//! Tauri shell is what calls it. What this proves is everything a headless
//! machine can prove about the arrangement —
//!
//! - the instance assembles, bootstraps an administrator and mints a gateway
//!   key on a database that did not exist a moment ago;
//! - the embedded axum data plane is listening on loopback and answers
//!   `/healthz`;
//! - it **refuses an unkeyed data-plane request**, which is the whole reason
//!   the key exists: the IPC channel is a trust boundary and a loopback socket
//!   is not;
//! - it refuses `/admin/api`, because the management surfaces are on IPC;
//! - and an IPC command dispatched through the real
//!   [`invoke_handler`](gproxy_host_tauri::ipc::invoke_handler) returns real
//!   rows out of that database.
//!
//! The credential store is [`MemoryStore`], so nothing here touches a real
//! keychain, needs a D-Bus session or leaves anything behind on the machine
//! that ran it. `tests/table.rs` and `src/secrets.rs` cover the keychain's own
//! behaviour, including the fallback.

use gproxy_host_tauri::{Desktop, secrets::MemoryStore};
use tauri::{
    Manager,
    test::{INVOKE_KEY, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The origin an invoke has to come from to count as the application's own.
///
/// `tauri://localhost` rather than the `http://tauri.localhost` of Tauri's own
/// doc example: the two are the same origin on Windows and Android and *not*
/// on Linux and macOS, where the custom protocol keeps its scheme. A request
/// from anywhere else is treated as remote content and goes through the ACL,
/// which is the check this crate relies on — so getting it wrong here would
/// have the test asserting on a permission refusal instead of on an answer.
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// One HTTP/1.1 request, written and read by hand.
///
/// A client crate would bring a proxy resolver that reads `HTTP_PROXY` out of
/// whatever shell ran the test and send these somewhere other than the socket
/// under test.
async fn request(address: std::net::SocketAddr, head: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(format!("{head}\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// A data directory with a `gproxy.toml` that asks for an ephemeral port, so
/// two tests can run at once and neither fights the fixed default.
fn data_dir() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("gproxy.toml"), "port = 0\n").unwrap();
    directory
}

#[tokio::test(flavor = "multi_thread")]
async fn the_data_plane_answers_on_loopback_and_refuses_an_unkeyed_request() {
    let data = data_dir();
    let desktop = Desktop::start(data.path().to_path_buf(), &MemoryStore::default())
        .await
        .expect("the instance should start on an empty data directory");
    let address = desktop.data_plane().address;
    assert!(address.ip().is_loopback(), "{address}");
    assert_ne!(address.port(), 0, "an ephemeral port should have resolved");

    let healthz = request(address, "GET /healthz HTTP/1.1").await;
    assert_eq!(status(&healthz), 200, "{healthz}");
    assert!(healthz.contains("\"status\":\"ok\""), "{healthz}");

    // The point of the gateway key. Every process on this machine can reach
    // this socket; none of them may spend the user's upstream quota.
    let unkeyed = request(address, "POST /v1/messages HTTP/1.1").await;
    assert_eq!(status(&unkeyed), 401, "{unkeyed}");

    // And the management surfaces are not on the socket at all.
    let admin = request(address, "GET /admin/api/users HTTP/1.1").await;
    assert_eq!(status(&admin), 404, "{admin}");
    assert!(admin.contains("Tauri IPC"), "{admin}");

    desktop.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_gateway_key_is_kept_and_reused_across_a_restart() {
    let data = data_dir();
    let store = MemoryStore::default();

    let first = Desktop::start(data.path().to_path_buf(), &store)
        .await
        .unwrap();
    let key = first.data_plane().gateway_key.clone();
    assert!(!key.is_empty());
    assert!(first.secrets().secrets_are_sealed());
    first.shutdown();

    // A second start over the same directory must not mint a second
    // administrator, a second key, or a second master key.
    let second = Desktop::start(data.path().to_path_buf(), &store)
        .await
        .unwrap();
    assert_eq!(second.data_plane().gateway_key, key);
    second.shutdown();
}

/// The command table, over Tauri's mock runtime.
///
/// This is the real [`gproxy_host_tauri::ipc::invoke_handler`] and the real
/// generated commands; only the window is a fake.
fn app(desktop: Desktop) -> tauri::App<tauri::test::MockRuntime> {
    let app = mock_builder()
        .invoke_handler(gproxy_host_tauri::ipc::invoke_handler())
        .build(mock_context(noop_assets()))
        .expect("the mock app should build");
    app.manage(desktop);
    app
}

fn invoke(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    command: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let response = tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: LOCAL_ORIGIN.parse().unwrap(),
            body: body.into(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .unwrap_or_else(|error| panic!("`{command}` failed: {error:?}"));
    response.deserialize().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ipc_command_returns_real_rows() {
    let data = data_dir();
    let desktop = Desktop::start(data.path().to_path_buf(), &MemoryStore::default())
        .await
        .unwrap();
    let plane = desktop.data_plane().clone();

    // The mock runtime wants the main thread's blocking world, and the
    // instance is already running on this test's runtime.
    let answers = tokio::task::spawn_blocking(move || {
        let app = app(desktop);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        let mut answers = Vec::new();
        // The shell's own state.
        answers.push(invoke(
            &window,
            "desktop_instance_status",
            serde_json::json!({}),
        ));
        // An `admin` command: `Operations::users().list(..)`, against the
        // administrator bootstrap just created.
        answers.push(invoke(
            &window,
            "admin_users_list",
            serde_json::json!({ "query": {} }),
        ));
        // A `manage` command reading the engine's own configuration.
        answers.push(invoke(
            &window,
            "manage_providers_list",
            serde_json::json!({ "query": {} }),
        ));
        // A `portal` command, scoped to the local administrator by
        // construction — note that nothing in the request names a user.
        answers.push(invoke(&window, "portal_me_context", serde_json::json!({})));
        // A synchronous one from the `@manual` block.
        answers.push(invoke(
            &window,
            "manage_catalog_channels",
            serde_json::json!({}),
        ));
        answers.push(invoke(
            &window,
            "admin_audit_query",
            serde_json::json!({ "query": {} }),
        ));
        answers
    })
    .await
    .unwrap();

    let [status, users, providers, context, channels, audit] = answers.as_slice() else {
        panic!("expected six answers");
    };

    assert_eq!(status["port"], plane.address.port());
    assert_eq!(status["baseUrl"], plane.base_url);
    assert_eq!(status["secretsAreSealed"], true);

    // The administrator `bootstrap` created, read back through the command
    // table rather than out of the database.
    assert_eq!(users["total"], 1);
    assert_eq!(users["items"][0]["name"], "desktop");
    assert_eq!(users["items"][0]["role"], "admin");

    // An empty instance really has no providers — and answering `0` is a
    // different thing from the command failing.
    assert_eq!(providers["total"], 0);

    assert_eq!(context["user"]["name"], "desktop");
    assert_eq!(
        audit["total"], 5,
        "generated and manual IPC reads are audited"
    );
    assert!(
        audit["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "manage.catalog.channels")
    );

    // Every channel this build was compiled with.
    assert!(
        channels.as_array().is_some_and(|list| !list.is_empty()),
        "{channels}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_ipc_command_answers_the_product_error_envelope() {
    let data = data_dir();
    let desktop = Desktop::start(data.path().to_path_buf(), &MemoryStore::default())
        .await
        .unwrap();

    let error = tokio::task::spawn_blocking(move || {
        let app = app(desktop);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        tauri::test::get_ipc_response(
            &window,
            InvokeRequest {
                cmd: "admin_users_get".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: LOCAL_ORIGIN.parse().unwrap(),
                body: serde_json::json!({ "id": "nobody" }).into(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .expect_err("a user that does not exist is not a success")
    })
    .await
    .unwrap();

    // The rejection is the `IpcError` envelope, unwrapped: Tauri has already
    // framed it as the failure, so there is no second `{ "error": … }` around
    // it the way the HTTP host needs one.
    assert_eq!(error["code"], "not_found", "{error}");
    assert_eq!(error["status"], 404, "{error}");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("nobody")),
        "{error}"
    );
}

/// The console in the window: its `fetch`, through the server's own router,
/// authenticated as the local administrator without the page naming anyone.
#[tokio::test(flavor = "multi_thread")]
async fn the_console_request_is_answered_by_the_server_router() {
    let data = data_dir();
    let desktop = Desktop::start(data.path().to_path_buf(), &MemoryStore::default())
        .await
        .unwrap();

    let answers = tokio::task::spawn_blocking(move || {
        let app = app(desktop);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let console = |method: &str, path: &str, body: Option<&str>| {
            invoke(
                &window,
                "desktop_console_request",
                serde_json::json!({ "request": {
                    "method": method,
                    "path": path,
                    // A cookie the page tried to send is dropped, not believed.
                    "headers": [["content-type", "application/json"], ["cookie", "gproxy_session=forged"]],
                    "body": body,
                } }),
            )
        };
        let answers = vec![
            console("GET", "/portal/api/context", None),
            console("GET", "/admin/api/context", None),
            console("POST", "/admin/api/export", Some(r#"{"includeSecrets":false}"#)),
            console("GET", "/admin/api/nothing-here", None),
        ];
        let refused = tauri::test::get_ipc_response(
            &window,
            InvokeRequest {
                cmd: "desktop_console_request".into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: LOCAL_ORIGIN.parse().unwrap(),
                body: serde_json::json!({ "request": { "method": "POST", "path": "/v1/messages" } })
                    .into(),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .expect_err("the data plane is not a console surface");
        (answers, refused)
    })
    .await
    .unwrap();
    let (answers, refused) = answers;
    let [portal, admin, export, missing] = answers.as_slice() else {
        panic!("expected four answers");
    };

    let body = |answer: &serde_json::Value| -> serde_json::Value {
        serde_json::from_str(answer["body"].as_str().unwrap()).unwrap()
    };
    assert_eq!(portal["status"], 200, "{portal}");
    assert_eq!(body(portal)["user"]["name"], "desktop", "{portal}");
    assert_eq!(admin["status"], 200, "{admin}");
    assert_eq!(export["status"], 200, "{export}");
    assert!(body(export)["formatVersion"].is_number(), "{export}");
    assert_eq!(missing["status"], 404, "{missing}");
    assert_eq!(refused["code"], "invalid_request", "{refused}");
}
