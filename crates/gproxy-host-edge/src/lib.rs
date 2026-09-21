//! The Cloudflare Workers host: a `fetch` handler over `gproxy-host-axum`'s
//! router.
//!
//! # The route table is not here
//!
//! This crate mounts [`gproxy_host_axum::router`] — the same `axum::Router`,
//! with the same routes, that the native binary serves — inside a Worker's
//! fetch handler. There is no second dispatch table, no list of paths, and no
//! `match` over segments. A route that the axum host grows is a route this
//! host serves, without an edit here.
//!
//! That was the open question this crate had to answer, and the answer is
//! empirical rather than theoretical. What makes it work:
//!
//! - `axum` builds for `wasm32-unknown-unknown` with `default-features =
//!   false`. Its server halves (`http1`/`http2`, which are hyper, and `tokio`,
//!   which is `axum::serve` and `ConnectInfo`) are features, and `Router`
//!   itself is target-independent.
//! - `worker`'s `http` feature makes `#[event(fetch)]` speak
//!   `http::Request<worker::Body>` and accept any `http_body::Body`, and
//!   `Router<()>` implements `tower::Service<http::Request<B>>`. So the two
//!   meet with no adapter in between — which is why this crate does **not**
//!   port v3's hand-written `web_sys::Request`/`Response`/`ReadableStream`
//!   adapters. They were right for v3, which spoke web-sys directly; here they
//!   would be a second copy of code Cloudflare maintains.
//! - the one real obstacle was `Send`. On wasm the engine below is `!Send` on
//!   purpose (a JS transport handle belongs to the isolate that made it, and
//!   `gproxy-client`'s `ClientBounds` says exactly that), while axum requires
//!   `S: Clone + Send + Sync + 'static` for a router's state and
//!   `Handler::Future: Send` for every handler. Two changes bridge it, both
//!   runtime-checked rather than asserted, both sound because an isolate is
//!   single-threaded: `gproxy_app::App` holds its handle in a `SendWrapper`,
//!   and `gproxy_host_axum::send` wraps each handler's body.
//!
//! # What this host does not serve
//!
//! | Surface | Why |
//! |---|---|
//! | websocket / realtime | a Worker upgrades with a `WebSocketPair`, which has no `http::Response` shape; the handshake is refused with `501` |
//! | the embedded console | the bundle belongs in Workers Assets, in front of this Worker, not inside a size-limited binary |
//!
//! Everything else — the data plane, the OAuth issuer, `/admin/api`,
//! `/portal/api`, `/publications/{id}` and `/healthz` — is the axum router's,
//! unchanged.
//!
//! # The shape of a request
//!
//! 1. [`instance::instance`] returns the isolate's assembly, building it on
//!    the first request that needs one;
//! 2. [`Assembled::tick`](instance::Assembled::tick) catches up on
//!    configuration written by another instance — the whole of synchronization
//!    at the edge, because nothing runs between requests;
//! 3. the router answers.
//!
//! Streaming needs nothing extra here. A response body that is still running
//! when the handler returns keeps the request's lease, capture and settlement
//! alive inside itself — `gproxy-host-axum`'s `LeasedBody` owns them — and the
//! Workers runtime keeps the isolate alive until the stream it is writing ends.

// A Worker isolate is single-threaded and the JS transport handles held
// through `dyn LibsqlTransport` are thread-bound; shared ownership still goes
// through `Arc` so the assembly reads the same on every target. The same allow
// is on `gproxy-core` and `gproxy-client` for the same reason.
#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

pub mod config;

// A Worker with no store backend would compile, deploy, and then fail to
// assemble on its first request — the one failure mode this crate spends its
// whole README avoiding. `config.rs` is plain data and still builds on the
// host target with no features, which is what its tests run against.
#[cfg(all(target_arch = "wasm32", not(any(feature = "d1", feature = "libsql"))))]
compile_error!(
    "gproxy-host-edge needs somewhere to keep its configuration: enable the `d1` feature (the \
     default) or `libsql`"
);

#[cfg(target_arch = "wasm32")]
pub mod instance;

#[cfg(target_arch = "wasm32")]
mod entry {
    use tower::Service;
    use worker::{Context, Env, HttpRequest, event};

    /// The Worker's entry point.
    ///
    /// Deliberately thin: everything it could decide is decided below it, and
    /// the two things it does that the native host's `main` also does are to
    /// assemble once and to answer. The request is handed to the router as it
    /// arrives — `worker::Body` is an `http_body::Body`, so no bytes are
    /// buffered on the way in and a streamed upload reaches an upstream as a
    /// stream.
    #[event(fetch)]
    async fn fetch(
        request: HttpRequest,
        env: Env,
        _ctx: Context,
    ) -> worker::Result<http::Response<axum::body::Body>> {
        let instance = crate::instance::instance(&env).await?;
        instance.tick().await;
        let mut router = instance.router();
        // Infallible: `Router`'s error type is `Infallible`, which is the
        // whole reason every refusal in this gateway is a `Response` rather
        // than an `Err`.
        match router.call(request).await {
            Ok(response) => Ok(response),
            Err(infallible) => match infallible {},
        }
    }
}
