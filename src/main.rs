//! GPROXY v2 binary: parse CLI/env config, wire persistence + state + router, serve.

#[path = "main/bootstrap.rs"]
mod bootstrap;
#[path = "main/cli.rs"]
mod cli;
#[path = "main/server_lifecycle.rs"]
mod server_lifecycle;
#[cfg(windows)]
#[path = "main/windows_process.rs"]
mod windows_process;
#[cfg(windows)]
#[path = "main/windows_tray.rs"]
mod windows_tray;

use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server_lifecycle::init_tracing();
    bootstrap::run(cli::Cli::parse()).await
}
