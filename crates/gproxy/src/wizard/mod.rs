//! The first run in a terminal: the desktop wizard's questions, asked by the
//! `gproxy` command itself.
//!
//! # When it runs
//!
//! Only when there is nobody to ask but a person and nothing has been decided
//! yet: no subcommand, a terminal on both ends, no `.env`, none of the flags
//! or variables that pick a store, a directory or an administrator password,
//! and no database where the defaults would put one. A container, a service
//! unit or a `nohup` has no terminal and keeps today's behaviour — the
//! administrator is created and its credentials printed once.
//!
//! # What it leaves behind
//!
//! The answers are written to `.env`, which every later `gproxy` in this
//! directory loads before parsing its arguments and which `service install`
//! points the unit at. A fresh master key goes there too, so credential
//! secrets are sealed from the first one. The password is not written
//! anywhere: it is hashed into the database and forgotten.

pub mod i18n;

use std::{io::IsTerminal, path::Path, sync::Arc, time::Duration};

use gproxy_app::{App, config::StoreBackendConfig};
use gproxy_sdk::{
    ChannelDescriptor, LoginMode,
    dto::{AuthCodeComplete, AuthCodeStart, DevicePollOutcome, DeviceStart, ProviderDto},
};
use gproxy_seaorm::BatchConnectionTrait;
use inquire::{
    Confirm, InquireError, MultiSelect, Password, PasswordDisplayMode, Select, Text,
    validator::Validation,
};

use crate::{
    Cli, Error, Result, Settings, bootstrap,
    cli::{self, ImportModeArg, ServiceAction},
    config::{self, AdminOptions},
    env, instance, serve, service, setup, transfer, update,
};
use i18n::{Lang, Msg};

/// Whether this invocation is a first run the wizard should take over.
pub fn wanted(cli: &Cli, settings: &Settings) -> bool {
    let options = &cli.options;
    cli.command.is_none()
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && options.config.is_none()
        && [
            &options.dsn,
            &options.persistence,
            &options.data_dir,
            &options.admin_password,
        ]
        .iter()
        .all(|value| value.as_deref().is_none_or(str::is_empty))
        && !env::path().exists()
        && match &settings.config.store {
            StoreBackendConfig::Sqlite { path } => {
                !database_file(settings.config.data_dir.as_deref(), path).exists()
            }
            _ => false,
        }
}

/// Ask, set the instance up, then serve — in this terminal or as a service.
pub async fn run(mut cli: Cli, settings: Settings, updates: update::UpdateOptions) -> Result<()> {
    match wizard(&mut cli, settings).await {
        Ok(Next::Serve(lang)) => {
            println!("{}\n", lang.t(Msg::Foreground));
            serve::run(config::settings(&cli)?, updates).await
        }
        Ok(Next::Done) => Ok(()),
        Err(Error::Other(message)) if message == CANCELLED => Ok(()),
        Err(error) => Err(error),
    }
}

enum Next {
    Serve(Lang),
    Done,
}

/// The sentinel an interrupted prompt unwinds with; [`run`] turns it into a
/// quiet exit rather than an error.
const CANCELLED: &str = "\u{0}setup cancelled";

/// Esc inside one credential's prompts: back to that credential's choices.
const BACK: &str = "\u{0}back";

enum Upstreams {
    Channels(Vec<ChannelDescriptor>),
    Import { path: String, key: Option<String> },
    Later,
}

async fn wizard(cli: &mut Cli, defaults: Settings) -> Result<Next> {
    let detected = Lang::detect();
    let lang = Select::new(detected.t(Msg::Language), Lang::ALL.to_vec())
        .with_help_message(detected.t(Msg::SelectHelp))
        .with_starting_cursor(Lang::ALL.iter().position(|l| *l == detected).unwrap_or(0))
        .prompt()
        .map_err(|error| cancelled(detected, error))?;
    let env_file = std::path::absolute(env::path()).unwrap_or_else(|_| env::path());
    println!("\n{}\n", lang.f(Msg::Welcome, env_file.display()));

    // Connection. Nothing is written until every question has an answer.
    heading(lang.t(Msg::Connection));
    let host = Text::new(lang.t(Msg::Host))
        .with_default(&defaults.config.host)
        .with_help_message(lang.t(Msg::HostHelp))
        .with_validator(move |text: &str| {
            Ok(match text.trim().parse::<std::net::IpAddr>() {
                Ok(_) => Validation::Valid,
                Err(_) => Validation::Invalid(lang.t(Msg::HostInvalid).into()),
            })
        })
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let port = Text::new(lang.t(Msg::Port))
        .with_default(&defaults.config.port.to_string())
        .with_validator(move |text: &str| {
            Ok(match text.trim().parse::<u16>() {
                Ok(port) if port > 0 => Validation::Valid,
                _ => Validation::Invalid(lang.t(Msg::PortInvalid).into()),
            })
        })
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let port: u16 = port.trim().parse().unwrap_or(defaults.config.port);
    let host = setup::listen(&host, port)?;
    let data_dir = Text::new(lang.t(Msg::DataDir))
        .with_default(defaults.config.data_dir.as_deref().unwrap_or("data"))
        .with_validator(move |text: &str| {
            Ok(if text.trim().is_empty() {
                Validation::Invalid(lang.t(Msg::DirectoryRequired).into())
            } else {
                Validation::Valid
            })
        })
        .prompt()
        .map_err(|error| cancelled(lang, error))?
        .trim()
        .to_owned();
    let kinds = setup::database_kinds();
    let names: Vec<&str> = kinds
        .iter()
        .map(|kind| match *kind {
            "postgres" => "PostgreSQL",
            "mysql" => "MySQL",
            _ => "SQLite",
        })
        .collect();
    let kind = if kinds.len() > 1 {
        let name = Select::new(lang.t(Msg::Database), names.clone())
            .with_help_message(lang.t(Msg::SelectHelp))
            .prompt()
            .map_err(|error| cancelled(lang, error))?;
        kinds[names.iter().position(|n| *n == name).unwrap_or(0)]
    } else {
        "sqlite"
    };
    let dsn = if kind == "sqlite" {
        Text::new(lang.t(Msg::DatabaseFile))
            .with_default("gproxy.db")
            .prompt()
            .map_err(|error| cancelled(lang, error))?
            .trim()
            .to_owned()
    } else {
        let placeholder = format!(
            "{kind}://user:password@host:{}/gproxy",
            if kind == "mysql" { 3306 } else { 5432 }
        );
        Text::new(lang.t(Msg::DatabaseUrl))
            .with_placeholder(&placeholder)
            .with_validator(move |text: &str| {
                let store = StoreBackendConfig::Url {
                    dsn: text.trim().to_owned(),
                };
                let scheme_matches = text.trim().starts_with(if kind == "mysql" {
                    "mysql://"
                } else {
                    "postgres"
                });
                Ok(if scheme_matches && setup::database(&store).is_ok() {
                    Validation::Valid
                } else {
                    Validation::Invalid(lang.t(Msg::DatabaseInvalid).into())
                })
            })
            .prompt()
            .map_err(|error| cancelled(lang, error))?
            .trim()
            .to_owned()
    };

    // Administrator.
    heading(lang.t(Msg::Account));
    let user = Text::new(lang.t(Msg::AdminUser))
        .with_default(&defaults.admin.user)
        .with_validator(move |text: &str| {
            Ok(match setup::admin_name(text) {
                Ok(_) => Validation::Valid,
                Err(_) => Validation::Invalid(lang.t(Msg::UserRequired).into()),
            })
        })
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let user = setup::admin_name(&user)?;
    let password = Password::new(lang.t(Msg::Password))
        .with_display_mode(PasswordDisplayMode::Masked)
        .with_custom_confirmation_message(lang.t(Msg::Confirmation))
        .with_custom_confirmation_error_message(lang.t(Msg::PasswordMismatch))
        .with_validator(move |text: &str| {
            Ok(match setup::password(text) {
                Ok(()) => Validation::Valid,
                Err(_) => Validation::Invalid(lang.t(Msg::PasswordInvalid).into()),
            })
        })
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let api_key = Password::new(lang.t(Msg::InitialKey))
        .with_display_mode(PasswordDisplayMode::Masked)
        .with_help_message(lang.t(Msg::GenerateKey))
        .without_confirmation()
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let api_key = Some(api_key.trim().to_owned()).filter(|key| !key.is_empty());

    // Upstreams.
    heading(lang.t(Msg::Upstreams));
    let choices = vec![
        lang.t(Msg::ChooseChannels),
        lang.t(Msg::ImportFile),
        lang.t(Msg::Later),
    ];
    let choice = Select::new(lang.t(Msg::UpstreamChoice), choices.clone())
        .with_help_message(lang.t(Msg::SelectHelp))
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    let upstreams = if choice == choices[0] {
        let catalog: Vec<ChannelOption> = setup::channels()
            .into_iter()
            .map(|descriptor| ChannelOption { lang, descriptor })
            .collect();
        let picked = MultiSelect::new(lang.t(Msg::Channels), catalog)
            .with_help_message(lang.t(Msg::ChannelsHelp))
            .with_page_size(12)
            .prompt()
            .map_err(|error| cancelled(lang, error))?;
        Upstreams::Channels(picked.into_iter().map(|option| option.descriptor).collect())
    } else if choice == choices[1] {
        let path = Text::new(lang.t(Msg::ImportPath))
            .with_validator(move |text: &str| {
                Ok(if Path::new(text.trim()).is_file() {
                    Validation::Valid
                } else {
                    Validation::Invalid(lang.t(Msg::FileMissing).into())
                })
            })
            .prompt()
            .map_err(|error| cancelled(lang, error))?
            .trim()
            .to_owned();
        let key = Password::new(lang.t(Msg::SourceKey))
            .with_display_mode(PasswordDisplayMode::Masked)
            .with_help_message(lang.t(Msg::SourceKeyHelp))
            .without_confirmation()
            .prompt()
            .map_err(|error| cancelled(lang, error))?;
        Upstreams::Import {
            path,
            key: Some(key.trim().to_owned()).filter(|key| !key.is_empty()),
        }
    } else {
        Upstreams::Later
    };

    // Every answer is in. Layer them exactly as the next start will see them
    // from `.env`, so this process and that one open the same instance.
    println!("\n{}", lang.t(Msg::Starting));
    let master_key = master_key()?;
    let options = &mut cli.options;
    options.host = Some(host.to_string());
    options.port = Some(port.to_string());
    options.data_dir = Some(data_dir.clone());
    options.dsn = Some(dsn.clone());
    options.master_key = Some(master_key.clone());
    let mut settings = config::settings(cli)?;
    setup::probe(host, port).map_err(Error::other)?;
    settings.admin = AdminOptions {
        user: user.clone(),
        password: Some(password),
        api_key: api_key.clone(),
    };

    let instance = instance::open(&settings, instance::OpenOptions::management()).await?;
    let report = bootstrap::ensure_admin(&instance.app, &settings.admin).await?;
    let key = match &report {
        bootstrap::Report::Created {
            api_key: minted, ..
        } => minted.clone().or(api_key),
        _ => {
            println!("{}", lang.t(Msg::ExistingAdmin));
            None
        }
    };
    write_env(
        &env_file,
        &[
            (cli::HOST, host.to_string()),
            (cli::PORT, port.to_string()),
            (cli::DATA_DIR, data_dir),
            (cli::DSN, dsn),
            (cli::MASTER_KEY, master_key),
        ],
    )?;
    println!("{}", lang.f(Msg::Saved, env_file.display()));

    match upstreams {
        Upstreams::Channels(picked) if !picked.is_empty() => {
            let ids: Vec<String> = picked.iter().map(|d| d.id.to_owned()).collect();
            let providers = setup::create_providers(&instance.app, &ids).await?;
            for provider in &providers {
                if let Some(descriptor) = picked.iter().find(|d| d.id == provider.channel) {
                    credentials(lang, &instance.app, provider, descriptor).await?;
                }
            }
        }
        Upstreams::Import { path, key } => {
            if let Err(error) = transfer::import(
                &instance.app,
                Path::new(&path),
                ImportModeArg::Merge,
                key.as_deref(),
            )
            .await
            {
                println!("{}", lang.f(Msg::Failed, error));
            }
        }
        _ => {}
    }
    instance.app.gproxy().shutdown();

    // Ready.
    let base = local_base_url(host, port);
    heading(lang.t(Msg::Ready));
    println!("  {:<18} {base}", lang.t(Msg::BaseUrl));
    if let Some(key) = &key {
        println!("  {:<18} {key}", lang.t(Msg::GatewayKey));
    }
    if settings.config.console.enabled {
        println!("  {:<18} {base}/console  ({user})", lang.t(Msg::Console));
    }
    if key.is_some() {
        println!("\n{}", lang.t(Msg::KeyShownOnce));
    }
    println!();

    let install = Confirm::new(lang.t(Msg::Service))
        .with_default(false)
        .prompt()
        .map_err(|error| cancelled(lang, error))?;
    if install {
        // The unit reads `.env`; the settings it is resolved from must not
        // carry the password this process was given.
        let settings = config::settings(cli)?;
        service::run(&ServiceAction::Install { autostart: None }, &settings, None)?;
        return Ok(Next::Done);
    }
    Ok(Next::Serve(lang))
}

/// One credential for `provider`, offered the ways its channel accepts. Each
/// attempt that fails is reported and offered again; skipping is always one
/// of the choices.
async fn credentials<C>(
    lang: Lang,
    app: &Arc<App<C>>,
    provider: &ProviderDto,
    descriptor: &ChannelDescriptor,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let name = provider.display_name.as_deref().unwrap_or(&provider.name);
    heading(&lang.f(Msg::Credentials, format!("{name} (/{})", provider.name)));
    let takes_key = descriptor.credential_kinds.contains(&"api_key");
    let login = descriptor
        .login_modes
        .iter()
        .copied()
        .find(|mode| *mode != LoginMode::ApiKey);
    if !takes_key && login.is_none() {
        let kinds: Vec<&str> = descriptor
            .credential_kinds
            .iter()
            .map(|kind| i18n::credential_kind(lang, kind))
            .collect();
        println!("{}", lang.f(Msg::LaterInConsole, kinds.join(" / ")));
        return Ok(());
    }
    let mut choices = Vec::new();
    if takes_key {
        choices.push(lang.t(Msg::PasteKey));
    }
    if login.is_some() {
        choices.push(lang.t(Msg::SignIn));
    }
    choices.push(lang.t(Msg::Skip));
    loop {
        // Esc here skips this provider; only Ctrl+C ends the wizard.
        let choice = match Select::new(lang.t(Msg::CredentialAction), choices.clone())
            .with_help_message(lang.t(Msg::SelectHelp))
            .prompt()
        {
            Ok(choice) => choice,
            Err(InquireError::OperationCanceled) => return Ok(()),
            Err(error) => return Err(cancelled(lang, error)),
        };
        let outcome = if choice == lang.t(Msg::PasteKey) {
            match Password::new(lang.t(Msg::ApiKey))
                .with_display_mode(PasswordDisplayMode::Masked)
                .without_confirmation()
                .prompt()
            {
                Ok(key) => setup::add_api_key(app, provider, &key)
                    .await
                    .map_err(Error::from),
                Err(error) => Err(back(lang, error)),
            }
        } else if choice == lang.t(Msg::SignIn) {
            match login {
                Some(LoginMode::DeviceCode) => device_login(lang, app, provider).await,
                _ => authcode_login(lang, app, provider).await,
            }
        } else {
            return Ok(());
        };
        match outcome {
            Ok(()) => {
                println!("{}", lang.t(Msg::Added));
                return Ok(());
            }
            Err(Error::Other(message)) if message == CANCELLED => {
                return Err(Error::Other(message));
            }
            Err(Error::Other(message)) if message == BACK => {}
            Err(error) => println!("{}", lang.f(Msg::Failed, error)),
        }
    }
}

/// Print the authorization link, read back what the browser ended on. A
/// channel with a registered redirect takes the callback URL whole; one
/// without takes the bare code, with the state from the link it issued — the
/// console's login dialog makes the same distinction.
async fn authcode_login<C>(lang: Lang, app: &Arc<App<C>>, provider: &ProviderDto) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let login = app.gproxy().login();
    let started = login
        .authcode_start(AuthCodeStart {
            provider_id: provider.id.clone(),
            ..Default::default()
        })
        .await?;
    println!("{}\n\n  {}\n", lang.t(Msg::OpenLink), started.authorize_url);
    let pasted = Text::new(lang.t(Msg::PasteCallback))
        .prompt()
        .map_err(|error| back(lang, error))?;
    let pasted = pasted.trim().to_owned();
    let request = if started.redirect_uri.is_empty() {
        let state = reqwest::Url::parse(&started.authorize_url)
            .ok()
            .and_then(|url| {
                url.query_pairs()
                    .find(|(name, _)| name == "state")
                    .map(|(_, value)| value.into_owned())
            });
        AuthCodeComplete {
            login_session_id: started.login_session_id,
            code: Some(pasted),
            state,
            ..Default::default()
        }
    } else {
        AuthCodeComplete {
            login_session_id: started.login_session_id,
            callback_url: Some(pasted),
            ..Default::default()
        }
    };
    login.authcode_complete(request).await?;
    app.reload_all().await?;
    Ok(())
}

/// Print the code and where to enter it, then poll at the pace the upstream
/// asked for until it answers or the code expires.
async fn device_login<C>(lang: Lang, app: &Arc<App<C>>, provider: &ProviderDto) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let login = app.gproxy().login();
    let started = login
        .device_start(DeviceStart {
            provider_id: provider.id.clone(),
            ..Default::default()
        })
        .await?;
    let link = started
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&started.verification_uri);
    println!("{}\n\n  {link}\n", lang.t(Msg::OpenLink));
    println!("{}\n", lang.f(Msg::DeviceCode, &started.user_code));
    println!("{}", lang.t(Msg::Waiting));
    let mut interval = started.interval_secs;
    loop {
        tokio::time::sleep(Duration::from_secs(interval.max(1))).await;
        match login.device_poll(&started.login_session_id).await? {
            DevicePollOutcome::Pending { interval_secs } => interval = interval_secs,
            DevicePollOutcome::Ready { .. } => {
                app.reload_all().await?;
                return Ok(());
            }
            DevicePollOutcome::Denied => return Err(Error::other(lang.t(Msg::Denied))),
            DevicePollOutcome::Expired => return Err(Error::other(lang.t(Msg::Expired))),
        }
    }
}

/// A channel in the picker: its name, then how it authenticates.
struct ChannelOption {
    lang: Lang,
    descriptor: ChannelDescriptor,
}

impl std::fmt::Display for ChannelOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kinds: Vec<&str> = self
            .descriptor
            .credential_kinds
            .iter()
            .map(|kind| i18n::credential_kind(self.lang, kind))
            .collect();
        write!(
            f,
            "{}  ({})",
            self.descriptor.display_name,
            kinds.join(" / ")
        )
    }
}

/// Ctrl+C or Esc ends the wizard without an error; anything else is one.
fn cancelled(lang: Lang, error: InquireError) -> Error {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            println!("{}", lang.t(Msg::Cancelled));
            Error::Other(CANCELLED.to_owned())
        }
        other => Error::other(other.to_string()),
    }
}

/// Esc returns to the previous choice; Ctrl+C still ends the wizard.
fn back(lang: Lang, error: InquireError) -> Error {
    match error {
        InquireError::OperationCanceled => Error::Other(BACK.to_owned()),
        other => cancelled(lang, other),
    }
}

fn heading(text: &str) {
    println!("\n── {text} ──");
}

fn database_file(data_dir: Option<&str>, path: &str) -> std::path::PathBuf {
    let path = Path::new(path);
    match data_dir {
        Some(root) if !path.is_absolute() => Path::new(root).join(path),
        _ => path.to_path_buf(),
    }
}

/// Unspecified listeners are reachable through loopback on this machine.
fn local_base_url(host: std::net::IpAddr, port: u16) -> String {
    let host = match host {
        std::net::IpAddr::V4(ip) if ip.is_unspecified() => std::net::Ipv4Addr::LOCALHOST.into(),
        std::net::IpAddr::V6(ip) if ip.is_unspecified() => std::net::Ipv6Addr::LOCALHOST.into(),
        other => other,
    };
    format!("http://{}", std::net::SocketAddr::new(host, port))
}

/// 32 random bytes in standard base64, the shape `GPROXY_MASTER_KEY` reads.
fn master_key() -> Result<String> {
    use base64::Engine;
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| {
        Error::other("secure randomness is unavailable; cannot generate a master key")
    })?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// A new `.env`, readable by its owner only. Values are single-quoted, which
/// `dotenvy` reads literally; one containing a quote is double-quoted with the
/// three characters that are special there escaped.
fn write_env(path: &Path, values: &[(&str, String)]) -> Result<()> {
    use std::io::Write;
    let mut text = String::from("# Written by the gproxy first-run wizard.\n");
    for (name, value) in values {
        let quoted = if value.contains('\'') {
            format!(
                "\"{}\"",
                value
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('$', "\\$")
            )
        } else {
            format!("'{value}'")
        };
        text.push_str(&format!("{name}={quoted}\n"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| Error::io(format!("creating {}", path.display()), error))?;
    file.write_all(text.as_bytes())
        .map_err(|error| Error::io(format!("writing {}", path.display()), error))
}
