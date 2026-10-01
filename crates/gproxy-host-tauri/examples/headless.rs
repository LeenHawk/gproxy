//! Start a desktop instance without a window, and show what it does.
//!
//! The library holds every decision and the Tauri shell only calls it, which
//! means the whole arrangement can be driven from a plain `main` on a machine
//! with no display server. That is worth having as something you can run
//! rather than only as an assertion in `tests/assembly.rs`:
//!
//! ```sh
//! cargo run -p gproxy-host-tauri --example headless
//! ```
//!
//! It assembles an instance in a temporary directory, prints the loopback
//! address the data plane bound, speaks HTTP to it by hand, dispatches two IPC
//! commands through the real handler, and shuts down. The temporary directory
//! goes with it, so running this twice leaves nothing behind.
//!
//! The credential store is [`MemoryStore`], not the system keychain: an
//! example that wrote into somebody's login keyring to demonstrate itself
//! would be a rude example.

use gproxy_host_tauri::{Desktop, secrets::MemoryStore};
use tauri::{
    Manager,
    test::{INVOKE_KEY, mock_builder, mock_context, noop_assets},
    webview::InvokeRequest,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("gproxy_host_tauri=info,gproxy=info")
        .with_writer(std::io::stderr)
        .init();

    let data = tempfile::tempdir()?;
    // Port 0: an example must not fight whatever is already on 8787.
    std::fs::write(data.path().join("gproxy.toml"), "port = 0\n")?;

    let desktop = Desktop::start(data.path().to_path_buf(), &MemoryStore::default()).await?;
    let plane = desktop.data_plane().clone();
    println!("\n== the instance ==");
    println!("data directory  {}", data.path().display());
    println!("data plane      {}", plane.base_url);
    println!("secrets sealed  {}", desktop.secrets().secrets_are_sealed());

    println!("\n== the data plane, over HTTP ==");
    for (label, head) in [
        ("GET /healthz", "GET /healthz HTTP/1.1"),
        ("POST /v1/messages, no key", "POST /v1/messages HTTP/1.1"),
        (
            "GET /admin/api/users (on IPC instead)",
            "GET /admin/api/users HTTP/1.1",
        ),
    ] {
        println!("\n-- {label}");
        print!("{}", request(plane.address, head).await?);
    }

    println!("\n== the same instance, over IPC ==");
    let desktop_for_ipc = desktop.clone();
    let answers = tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        let app = mock_builder()
            .invoke_handler(gproxy_host_tauri::ipc::invoke_handler())
            .build(mock_context(noop_assets()))
            .map_err(|error| error.to_string())?;
        app.manage(desktop_for_ipc);
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .map_err(|error| error.to_string())?;
        ["desktop_instance_status", "admin_users_list"]
            .into_iter()
            .map(|command| invoke(&window, command))
            .collect()
    })
    .await??;
    for answer in answers {
        println!("{answer}");
    }

    desktop.shutdown();
    println!("\nstopped.");
    Ok(())
}

/// One HTTP/1.1 exchange, written and read by hand, head only.
async fn request(
    address: std::net::SocketAddr,
    head: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(format!("{head}\r\nHost: {address}\r\nConnection: close\r\n\r\n").as_bytes())
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

/// One IPC command through the real handler, pretty-printed.
fn invoke(
    window: &tauri::WebviewWindow<tauri::test::MockRuntime>,
    command: &str,
) -> Result<String, String> {
    let value: serde_json::Value = tauri::test::get_ipc_response(
        window,
        InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            // `tauri://localhost` is the application's own origin on Linux and
            // macOS. Anywhere else is remote content and goes through the ACL.
            url: "tauri://localhost".parse().map_err(|_| "bad url")?,
            body: serde_json::json!({ "query": {} }).into(),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{command}: {error}"))?
    .deserialize()
    .map_err(|error| error.to_string())?;
    Ok(format!(
        "\n-- {command}\n{}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
    ))
}
