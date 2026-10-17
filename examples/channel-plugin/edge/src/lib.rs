//! The stock GPROXY Worker plus [`Acme`](gproxy_example_channel::Acme): same
//! routes, same configuration bindings, same console, and one more channel.
//!
//! Build it from `deploy/cloudflare` of a gproxy checkout, which supplies the
//! `wrangler.toml`, the console assets and the post-build step:
//! `GPROXY_EDGE_CRATE=/absolute/path/to/this/crate pnpm run build`.

#[cfg(target_arch = "wasm32")]
mod entry {
    use std::sync::Arc;

    use worker::{Context, Env, HttpRequest, event};

    #[event(fetch)]
    async fn fetch(
        request: HttpRequest,
        env: Env,
        ctx: Context,
    ) -> worker::Result<gproxy_host_edge::Response> {
        gproxy_host_edge::fetch_with(request, env, ctx, || {
            vec![Arc::new(gproxy_example_channel::Acme)]
        })
        .await
    }
}
