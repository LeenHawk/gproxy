//! GPROXY v2 binary: parse CLI/env config, wire persistence + state + router, serve.

#[path = "main/bootstrap.rs"]
mod bootstrap;
#[path = "main/cli.rs"]
mod cli;
#[path = "main/server_lifecycle.rs"]
mod server_lifecycle;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server_lifecycle::init_tracing();
    bootstrap::run(cli::Cli::parse()).await
}
