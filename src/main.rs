use anyhow::Result;

use gproxy::context::AppContext;
use gproxy::route;

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    AppContext::init().await?;
    let ctx = AppContext::get();
    let config = ctx.get_config();
    let first_api_key = config.app.api_keys.first().map(String::as_str).unwrap_or("");
    println!("admin_key={}", config.app.admin_key);
    println!("api_key[0]={}", first_api_key);
    route::serve(ctx).await?;

    Ok(())
}
