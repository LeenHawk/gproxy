//! The GPROXY v4 command line.
//!
//! This crate is the only place in the workspace that reads the environment,
//! opens a file, binds a socket or prints to a terminal. Everything below it
//! is a library that takes configuration as an argument:
//! [`gproxy_sdk::GproxyBuilder`] assembles the engine, [`gproxy_app::App`]
//! holds identity and admission, and [`gproxy_host_axum::router`] turns the two
//! into an HTTP surface. What is left over — and what lives here — is the
//! operator's half of the product.
//!
//! # One configuration, resolved once
//!
//! Four sources produce one [`gproxy_app::AppConfig`], and the later a source
//! comes in this list the weaker it is:
//!
//! 1. the command line;
//! 2. the real process environment;
//! 3. `.env`, loaded without overriding anything the environment already set;
//! 4. the TOML file named by `--config`;
//! 5. `AppConfig::default()`.
//!
//! The first three collapse into one step, because every `clap` field in
//! [`cli::Cli`] carries `env = "GPROXY_…"`: by the time parsing is done, a flag
//! has already beaten an environment variable, and `.env` has already lost to a
//! real one. [`config::settings`] then overlays what is left onto the file.
//! That is deliberate — the `--help` output and the environment table in the
//! README are generated from the same attributes, so they cannot drift apart.
//!
//! The layering happens **once**, in [`run`], and the result is passed down. No
//! module below this one reads `std::env`.
//!
//! # The commands
//!
//! | Command | What it does |
//! |---|---|
//! | `serve` (default) | rotate if asked, bootstrap if the instance is new, bind, serve |
//! | `migrate` | synchronize the schema and exit |
//! | `bootstrap admin` | create the first administrator, idempotently |
//! | `export` | the instance's configuration as one JSON document |
//! | `import` | replay such a document, merging or replacing |
//! | `service` | install, remove or inspect the unit this machine's init system runs |

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod env;
pub mod error;
pub mod instance;
pub mod rotate;
pub mod serve;
pub mod service;
pub mod telemetry;
pub mod transfer;

pub use cli::{Cli, Command};
pub use config::Settings;
pub use error::{Error, Result};

/// Run one invocation, with the configuration layered exactly once.
///
/// Returns to the caller rather than exiting, so the process's exit code is
/// decided in one place and a failure is printed in one format.
pub async fn run(cli: Cli) -> Result<()> {
    let settings = config::settings(&cli)?;
    telemetry::init(&settings.telemetry)?;

    // Kept before the `match`, which moves `cli.command`. The unit `service
    // install` writes has to name the `--config` file this invocation read,
    // and `Settings` is the result of the layering — it has deliberately
    // forgotten which file produced it.
    let config_path = cli.options.config.clone();

    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve::run(settings).await,
        Command::Migrate => instance::migrate(&settings.config).await,
        Command::Bootstrap {
            target: cli::BootstrapTarget::Admin,
        } => {
            let instance = instance::open(&settings, instance::OpenOptions::management()).await?;
            let report = bootstrap::ensure_admin(&instance.app, &settings.admin).await?;
            report.announce();
            instance.app.gproxy().shutdown();
            Ok(())
        }
        Command::Export {
            out,
            include_secrets,
        } => {
            let instance = instance::open(&settings, instance::OpenOptions::management()).await?;
            transfer::export(&instance.app, &out, include_secrets).await?;
            instance.app.gproxy().shutdown();
            Ok(())
        }
        Command::Import {
            input,
            mode,
            source_master_key,
        } => {
            let instance = instance::open(&settings, instance::OpenOptions::management()).await?;
            transfer::import(&instance.app, &input, mode, source_master_key.as_deref()).await?;
            instance.app.gproxy().shutdown();
            Ok(())
        }
        // No database, no network, no runtime work: this one only reads the
        // resolved configuration and writes a file the init system will read.
        Command::Service { action } => service::run(&action, &settings, config_path.as_deref()),
    }
}
