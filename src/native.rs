//! Native CLI entrypoint reusable by custom GPROXY binaries.

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

/// Parse the standard GPROXY CLI and run the native server or subcommand.
///
/// Compile-time channel registrations are collected before bootstrap. A custom
/// binary normally only needs to retain its channel crates with
/// `use channel_crate as _;` and call this function.
pub async fn run_cli() -> anyhow::Result<()> {
    server_lifecycle::init_tracing();
    bootstrap::run(cli::Cli::parse()).await
}
