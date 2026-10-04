//! A restartable native host. Swift owns user consent and background task
//! lifetime; this library owns the existing gateway, never a VPN interface.

use std::{
    ffi::{CStr, CString, c_char},
    net::Shutdown,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    Extension,
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use gproxy::{
    Settings, bootstrap,
    config::{AdminOptions, TelemetryOptions},
    instance,
};
use gproxy_app::{
    App, AppConfig,
    config::{FileStorageConfig, MasterKey, MasterKeyConfig},
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpListener,
    runtime::Runtime,
    sync::oneshot,
    task::{JoinHandle, JoinSet},
};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static HOST: Mutex<Option<Session>> = Mutex::new(None);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Start {
    data_dir: String,
    master_key: String,
    password: String,
    api_key: String,
    port: u16,
    duration_seconds: u64,
}

struct Session {
    app: Arc<App<instance::Connection>>,
    stop: oneshot::Sender<()>,
    server: JoinHandle<()>,
    requests: Arc<AtomicU64>,
    base_url: String,
}

// Keep a shutdown handle alive through HTTP upgrades. Hyper hands upgraded
// WebSockets to detached pumps; aborting its connection task alone would not
// close those sockets. Weak handles avoid retaining completed connections.
struct SessionSocket {
    stream: tokio::net::TcpStream,
    _control: Arc<std::net::TcpStream>,
}

impl AsyncRead for SessionSocket {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buffer)
    }
}

impl AsyncWrite for SessionSocket {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

fn runtime() -> Result<&'static Runtime, String> {
    if let Some(runtime) = RUNTIME.get() {
        return Ok(runtime);
    }
    let built = Runtime::new().map_err(|error| error.to_string())?;
    Ok(RUNTIME.get_or_init(|| built))
}

async fn count(State(count): State<Arc<AtomicU64>>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if !["/admin/", "/portal/", "/console", "/healthz"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        count.fetch_add(1, Ordering::Relaxed);
    }
    next.run(request).await
}

async fn start(input: Start) -> Result<Session, String> {
    if !(1..=10_000).contains(&input.duration_seconds) {
        return Err("session duration must be between 1 and 10000 seconds".into());
    }
    if !std::path::Path::new(&input.data_dir).is_absolute() {
        return Err("data directory must be absolute".into());
    }
    let config = AppConfig {
        data_dir: Some(input.data_dir),
        port: input.port,
        master_key: MasterKeyConfig {
            key: MasterKey::Hex(input.master_key),
            ..Default::default()
        },
        file_storage: Some(FileStorageConfig::Fs {
            root: "files".into(),
        }),
        ..Default::default()
    };
    // Validate and reserve loopback before opening the database. No LAN bind,
    // route changes, VPN configuration, or environment-based credentials.
    config
        .master_key
        .resolve()
        .map_err(|error| error.to_string())?;
    if input.password.is_empty() || input.api_key.is_empty() {
        return Err("Keychain credentials are required".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", input.port))
        .await
        .map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    let settings = Settings {
        config,
        admin: AdminOptions {
            user: "admin".into(),
            password: Some(input.password),
            api_key: Some(input.api_key),
        },
        telemetry: TelemetryOptions::default(),
        instance_id: None,
    };
    let instance = instance::open(&settings, instance::OpenOptions::serving())
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = bootstrap::ensure_admin(&instance.app, &settings.admin).await {
        instance.app.shutdown();
        return Err(error.to_string());
    }
    let requests = Arc::new(AtomicU64::new(0));
    let router =
        gproxy_host_axum::router(gproxy_host_axum::HostState::new(instance.app.clone())).layer(
            axum::middleware::from_fn_with_state(requests.clone(), count),
        );
    let (stop, mut stopped) = oneshot::channel();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(input.duration_seconds);
    let app = instance.app.clone();
    // Own every connection in a JoinSet: expiration must also end streaming
    // connections, not just stop accepting new sockets.
    let server = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        let mut sockets: Vec<Weak<std::net::TcpStream>> = Vec::new();
        loop {
            tokio::select! {
                _ = &mut stopped => break,
                _ = tokio::time::sleep_until(deadline) => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = listener.accept() => {
                    let Ok((stream, peer)) = accepted else { break };
                    let Ok(stream) = stream.into_std() else { continue };
                    let Ok(control) = stream.try_clone() else { continue };
                    let Ok(stream) = tokio::net::TcpStream::from_std(stream) else { continue };
                    let control = Arc::new(control);
                    sockets.retain(|socket| socket.strong_count() > 0);
                    sockets.push(Arc::downgrade(&control));
                    let stream = SessionSocket { stream, _control: control };
                    let service = TowerToHyperService::new(router.clone().layer(Extension(ConnectInfo(peer))));
                    connections.spawn(async move {
                        let mut builder = Builder::new(TokioExecutor::new());
                        builder.http1().timer(TokioTimer::new()).header_read_timeout(gproxy_host_axum::serve::HEADER_READ_TIMEOUT);
                        builder.http2().enable_connect_protocol();
                        let _ = builder.serve_connection_with_upgrades(TokioIo::new(stream), service).await;
                    });
                }
            }
        }
        drop(listener);
        for socket in sockets.into_iter().filter_map(|socket| socket.upgrade()) {
            let _ = socket.shutdown(Shutdown::Both);
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        app.shutdown();
    });
    Ok(Session {
        app: instance.app,
        stop,
        server,
        requests,
        base_url: format!("http://{address}"),
    })
}

fn dispatch(command: &str, input: &str) -> Result<Value, String> {
    let runtime = runtime()?;
    // Calls come from Swift's bridge actor, not Tokio workers. Keeping this
    // gate through shutdown prevents a new session racing the old database.
    let mut host = HOST.lock().map_err(|error| error.to_string())?;
    match command {
        "start" => {
            if host.is_some() {
                return Err("a session is already active".into());
            }
            let input = serde_json::from_str(input).map_err(|error| error.to_string())?;
            *host = Some(runtime.block_on(start(input))?);
        }
        "stop" => {
            if let Some(session) = host.take() {
                runtime.block_on(async {
                    let _ = session.stop.send(());
                    let mut server = session.server;
                    if tokio::time::timeout(Duration::from_secs(5), &mut server)
                        .await
                        .is_err()
                    {
                        server.abort();
                        let _ = server.await;
                    }
                    session.app.shutdown();
                });
            }
        }
        "status" => {}
        _ => return Err("unknown native command".into()),
    }
    Ok(match host.as_ref() {
        Some(session) => {
            json!({ "running": !session.server.is_finished(), "baseUrl": session.base_url, "requests": session.requests.load(Ordering::Relaxed) })
        }
        None => json!({ "running": false, "requests": 0 }),
    })
}

/// Execute a command. The caller must release the returned JSON with
/// `gproxy_ios_free`; neither input pointer is retained.
///
/// # Safety
/// Both arguments must point to readable, NUL-terminated UTF-8 strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gproxy_ios_call(
    command: *const c_char,
    input: *const c_char,
) -> *mut c_char {
    let result = (|| {
        if command.is_null() || input.is_null() {
            return Err("null input".into());
        }
        let command = unsafe { CStr::from_ptr(command) }
            .to_str()
            .map_err(|error| error.to_string())?;
        let input = unsafe { CStr::from_ptr(input) }
            .to_str()
            .map_err(|error| error.to_string())?;
        dispatch(command, input)
    })();
    let response = match result {
        Ok(value) => json!({ "ok": true, "status": value }),
        Err(error) => json!({ "ok": false, "error": error }),
    };
    // JSON serialization escapes NUL characters, so this cannot contain one.
    CString::new(response.to_string())
        .expect("JSON has no literal NUL")
        .into_raw()
}

/// # Safety
/// `value` must be a live result of `gproxy_ios_call`, freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gproxy_ios_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpStream},
    };

    #[test]
    fn session_authentication_socket_cleanup_deadline_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        let mut options = json!({
            "dataDir": directory.path().to_str().unwrap(),
            "masterKey": "11".repeat(32),
            "password": "ios-test-password",
            "apiKey": "sk-ios-lifecycle-test",
            "port": 0,
            "durationSeconds": 10000,
        });
        let status = dispatch("start", &options.to_string()).unwrap();
        let address: SocketAddr = status["baseUrl"]
            .as_str()
            .unwrap()
            .trim_start_matches("http://")
            .parse()
            .unwrap();
        assert!(address.ip().is_loopback());
        assert!(dispatch("start", &options.to_string()).is_err());

        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .write_all(b"GET /v1/models HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 401"), "{response}");
        assert_eq!(dispatch("status", "{}").unwrap()["requests"], 1);

        // Leave a connection open with an unfinished header. Stopping must
        // terminate that connection too, and immediately release the port.
        let mut stalled = TcpStream::connect(address).unwrap();
        stalled
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stalled.write_all(b"GET /healthz HTTP/1.1\r\n").unwrap();
        assert_eq!(dispatch("stop", "{}").unwrap()["running"], false);
        let mut byte = [0];
        match stalled.read(&mut byte) {
            Ok(count) => assert_eq!(count, 0),
            Err(error) => assert!(matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            )),
        }
        assert!(TcpStream::connect(address).is_err());
        assert_eq!(dispatch("stop", "{}").unwrap()["running"], false);

        // Reuse the same DB and port in the same process. The native deadline
        // must close the listener without any Swift status polling or timer.
        options["port"] = json!(address.port());
        options["durationSeconds"] = json!(1);
        assert_eq!(
            dispatch("start", &options.to_string()).unwrap()["running"],
            true
        );
        std::thread::sleep(Duration::from_millis(1300));
        assert!(TcpStream::connect(address).is_err());
        assert_eq!(dispatch("status", "{}").unwrap()["running"], false);
        dispatch("stop", "{}").unwrap();

        options["durationSeconds"] = json!(10001);
        assert!(
            dispatch("start", &options.to_string())
                .unwrap_err()
                .contains("duration")
        );
        assert_eq!(dispatch("status", "{}").unwrap()["running"], false);
    }
}
