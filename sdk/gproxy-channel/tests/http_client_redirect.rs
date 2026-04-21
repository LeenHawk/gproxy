use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::routing::get;
use tokio::net::TcpListener;

use gproxy_channel::http_client::send_provider_request;

#[derive(Default)]
struct RedirectState {
    redirect_hits: AtomicUsize,
    wrong_path_hits: AtomicUsize,
    canonical_hits: AtomicUsize,
}

async fn redirect_handler(
    State(state): State<Arc<RedirectState>>,
) -> (
    StatusCode,
    [(header::HeaderName, &'static str); 1],
    &'static str,
) {
    state.redirect_hits.fetch_add(1, Ordering::Relaxed);
    (
        StatusCode::FOUND,
        [(header::LOCATION, "/v1/messages")],
        "redirecting",
    )
}

async fn wrong_path_handler(State(state): State<Arc<RedirectState>>) -> &'static str {
    state.wrong_path_hits.fetch_add(1, Ordering::Relaxed);
    "wrong path reached"
}

async fn canonical_target_handler(State(state): State<Arc<RedirectState>>) -> &'static str {
    state.canonical_hits.fetch_add(1, Ordering::Relaxed);
    "canonical path reached"
}

async fn start_redirect_server() -> (String, Arc<RedirectState>) {
    let state = Arc::new(RedirectState::default());
    let app = Router::new()
        .route("/v1/chat/completions", get(redirect_handler))
        .route("/v1/messages", get(wrong_path_handler))
        .route("/v1/chat/completions/", get(canonical_target_handler))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum::serve");
    });

    (format!("http://{addr}"), state)
}

#[tokio::test]
async fn provider_requests_block_cross_endpoint_redirects() {
    let (base_url, state) = start_redirect_server().await;

    let client = wreq::Client::builder()
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("client");

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!("{base_url}/v1/chat/completions"))
        .body(Vec::new())
        .expect("request");

    let response = send_provider_request(&client, request)
        .await
        .expect("response");

    assert_eq!(response.status, StatusCode::FOUND.as_u16());
    assert_eq!(state.redirect_hits.load(Ordering::Relaxed), 1);
    assert_eq!(
        state.wrong_path_hits.load(Ordering::Relaxed),
        0,
        "the HTTP client followed a redirect and hit the wrong path"
    );
}

#[tokio::test]
async fn provider_requests_allow_same_endpoint_canonicalization_redirects() {
    let state = Arc::new(RedirectState::default());
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            get(|| async {
                (
                    StatusCode::FOUND,
                    [(header::LOCATION, "/v1/chat/completions/")],
                    "redirecting",
                )
            }),
        )
        .route("/v1/chat/completions/", get(canonical_target_handler))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum::serve");
    });

    let client = wreq::Client::builder()
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("client");

    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(format!("http://{addr}/v1/chat/completions"))
        .body(Vec::new())
        .expect("request");

    let response = send_provider_request(&client, request)
        .await
        .expect("response");

    assert_eq!(response.status, StatusCode::OK.as_u16());
    assert_eq!(state.canonical_hits.load(Ordering::Relaxed), 1);
}
