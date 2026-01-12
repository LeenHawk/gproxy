use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;

use crate::admin;
use crate::context::AppContext;
use crate::public;
#[cfg(feature = "front")]
use crate::front;

pub fn app_router(ctx: Arc<AppContext>) -> Router {
    let mut router = Router::new()
        .nest("/admin", admin::router(ctx.clone()))
        .merge(public::router(ctx));
    #[cfg(feature = "front")]
    {
        router = router.merge(front::router());
    }
    router
}

pub async fn serve(ctx: Arc<AppContext>) -> anyhow::Result<()> {
    loop {
        let app = app_router(ctx.clone());
        let app_config = ctx.get_config();
        let addr = SocketAddr::new(app_config.app.host, app_config.app.port);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let mut shutdown_rx = ctx.reload_tx().subscribe();
        let shutdown_ctx = ctx.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
                let _ = shutdown_ctx.usage_store().flush().await;
            })
            .await?;
    }
}
