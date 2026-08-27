use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine as _;

pub(crate) struct Fixture {
    _directory: tempfile::TempDir,
    pub(crate) app: gproxy_app::AppHandle,
    server: gproxy_host_axum::AxumServer,
    pub(crate) client_key: String,
    pub(crate) quota_id: i64,
    stub_shutdown: tokio::sync::oneshot::Sender<()>,
    stub_task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

impl Fixture {
    pub(crate) async fn start() -> Self {
        let upstream_key = random_key();
        let client_key = random_key();
        let (upstream, stub_shutdown, stub_task) = start_stub(&upstream_key).await;
        let directory = tempfile::tempdir().expect("test directory");
        let data_dir = directory.path().join("data");
        let mut master_key = [0_u8; 32];
        getrandom::fill(&mut master_key).expect("master key randomness");
        let config = gproxy_app::Config::sqlite(
            "127.0.0.1:0".parse().unwrap(),
            data_dir,
            gproxy_app::MasterKeyConfig::new(Some(master_key)),
        );
        let listen_addr = config.listen_addr();
        let app = gproxy_app::App::start(config).await.expect("start app");
        let quota_id = crate::seed::operational(&app, upstream, upstream_key, &client_key).await;
        let server = gproxy_host_axum::AxumServer::bind(app.clone(), listen_addr)
            .await
            .expect("start axum host");
        Self {
            _directory: directory,
            app,
            server,
            client_key,
            quota_id,
            stub_shutdown,
            stub_task,
        }
    }

    pub(crate) fn gateway_url(&self) -> String {
        format!("http://{}/v1/chat/completions", self.server.local_addr())
    }

    pub(crate) async fn shutdown(self) {
        self.server.shutdown().await.expect("stop axum host");
        let _ = self.stub_shutdown.send(());
        self.stub_task
            .await
            .expect("join stub server")
            .expect("stop stub server");
    }
}

#[derive(Clone)]
struct StubState {
    authorization: HeaderValue,
}

async fn start_stub(
    upstream_key: &str,
) -> (
    std::net::SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<(), std::io::Error>>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind stub");
    let address = listener.local_addr().expect("stub address");
    let state = StubState {
        authorization: HeaderValue::from_str(&format!("Bearer {upstream_key}"))
            .expect("upstream authorization"),
    };
    let router = Router::new()
        .route("/v1/chat/completions", post(upstream))
        .with_state(state);
    let (shutdown, receiver) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = receiver.await;
            })
            .await
    });
    (address, shutdown, task)
}

async fn upstream(
    State(state): State<StubState>,
    headers: HeaderMap,
    Json(request): Json<serde_json::Value>,
) -> Response {
    if headers.get(http::header::AUTHORIZATION) != Some(&state.authorization) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if request["model"] != "upstream-model" {
        return StatusCode::BAD_REQUEST.into_response();
    }
    Json(serde_json::json!({
        "id": "chatcmpl-e2e",
        "object": "chat.completion",
        "created": 1,
        "model": "upstream-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "booted"},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    }))
    .into_response()
}

fn random_key() -> String {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).expect("key randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
