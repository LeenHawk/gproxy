//! First launch stays outside the running instance. Only non-secret choices
//! are persisted; the bootstrap password and import key are transient inputs.

use std::path::{Path, PathBuf};

use gproxy::config::AdminOptions;
use gproxy_app::{AppError, Operations, config::StoreBackendConfig};
use gproxy_sdk::dto::{ImportReportDto, ImportRequest};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::{IpcError, IpcResult, StartError, StartResult, config, engine};

pub const FILE: &str = "desktop-setup.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Choices {
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub admin_user: String,
    pub auto_start: bool,
    pub tray: bool,
    pub completed: bool,
    #[serde(skip)]
    pub database: StoreBackendConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub required: bool,
    pub can_choose_data_dir: bool,
    pub can_auto_start: bool,
    pub started: bool,
    pub database: StoreBackendConfig,
    pub database_kinds: Vec<&'static str>,
    #[serde(flatten)]
    pub choices: Choices,
}

// No Debug: neither a password nor a configuration document belongs in logs.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    pub data_dir: PathBuf,
    pub host: String,
    pub port: u16,
    pub admin_user: String,
    pub password: String,
    pub api_key: Option<String>,
    pub auto_start: bool,
    pub tray: bool,
    pub import: Option<ImportRequest>,
    pub database: StoreBackendConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub base_url: String,
    pub api_key: String,
    pub import_report: Option<ImportReportDto>,
    pub existing_admin_preserved: bool,
}

pub struct Setup {
    root: PathBuf,
    store: std::sync::Arc<dyn crate::secrets::SecretStore>,
    gate: tokio::sync::Mutex<()>,
}

impl Setup {
    pub fn new(root: PathBuf) -> Self {
        Self::with_store(root, std::sync::Arc::new(crate::secrets::Keychain))
    }

    pub fn with_store(
        root: PathBuf,
        store: std::sync::Arc<dyn crate::secrets::SecretStore>,
    ) -> Self {
        Self {
            root,
            store,
            gate: tokio::sync::Mutex::const_new(()),
        }
    }

    pub fn choices(&self) -> StartResult<Choices> {
        read_choices(&self.root)
    }

    pub fn status(&self) -> StartResult<SetupStatus> {
        let choices = self.choices()?;
        Ok(SetupStatus {
            required: !choices.completed,
            can_choose_data_dir: cfg!(desktop),
            can_auto_start: !cfg!(target_env = "ohos"),
            started: engine::started().is_some(),
            database: choices.database.clone(),
            database_kinds: database_kinds(),
            choices,
        })
    }

    pub async fn complete<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        mut request: SetupRequest,
    ) -> IpcResult<SetupResult> {
        let _gate = self.gate.lock().await;
        let previous = self.choices().map_err(failure)?;
        if previous.completed {
            return Err(AppError::Conflict("this instance is already set up".into()).into());
        }
        request.admin_user = request.admin_user.trim().to_owned();
        if request.admin_user.is_empty() {
            return Err(AppError::invalid("administrator name must not be blank").into());
        }
        gproxy_app::auth::password::validate(&request.password)?;
        let host: std::net::IpAddr = request
            .host
            .trim()
            .parse()
            .map_err(|_| AppError::invalid("enter a valid listening IP address"))?;
        request.host = host.to_string();
        if request.port == 0 {
            return Err(AppError::invalid("choose a port between 1 and 65535").into());
        }
        if !request.data_dir.is_absolute() {
            return Err(AppError::invalid("choose an absolute data directory path").into());
        }
        #[cfg(any(target_os = "android", target_env = "ohos"))]
        if request.data_dir != self.root {
            return Err(
                AppError::invalid("Mobile applications use their private data directory").into(),
            );
        }
        #[cfg(target_env = "ohos")]
        if request.auto_start || request.tray {
            return Err(AppError::invalid(
                "OpenHarmony does not support launch at login or a desktop tray",
            )
            .into());
        }
        validate_database(&request.database)?;
        let pending = self.root.join(FILE).is_file();
        let database_exists = config::database_file(&request.data_dir, &request.database)
            .is_some_and(|path| path.exists());
        if database_exists
            && (!pending
                || request.data_dir != previous.data_dir
                || request.database != previous.database)
        {
            return Err(AppError::Conflict("the selected directory already contains an instance; choose a new directory and import its configuration".into()).into());
        }
        if pending && database_exists && request.admin_user != previous.admin_user {
            return Err(AppError::Conflict(
                "finish setup with the administrator name already created for this instance".into(),
            )
            .into());
        }
        if let Some(desktop) = engine::started() {
            if desktop.data_dir() != request.data_dir
                || desktop.data_plane().address.port() != request.port
                || desktop.data_plane().address.ip() != host
                || desktop.app().config().store != request.database
            {
                return Err(AppError::Conflict("the instance has started; finish setup with its current data directory and port".into()).into());
            }
        } else {
            // Fail before creating a database when a server already owns this port.
            std::net::TcpListener::bind((host, request.port)).map_err(|error| {
                failure(format!(
                    "cannot listen on {}:{}: {error}",
                    host, request.port
                ))
            })?;
        }
        let mut choices = Choices {
            data_dir: request.data_dir,
            host: request.host,
            port: request.port,
            admin_user: request.admin_user,
            auto_start: request.auto_start,
            tray: request.tray,
            completed: false,
            database: request.database,
        };
        std::fs::create_dir_all(&choices.data_dir).map_err(failure)?;
        // A failed or interrupted setup remains a setup, even once SQLite exists.
        write_choices(&self.root, &choices).map_err(failure)?;
        write_config(
            &choices.data_dir,
            &choices.host,
            choices.port,
            &choices.database,
        )
        .map_err(failure)?;
        let desktop = engine::ensure_started_with_admin(
            &choices.data_dir,
            self.store.as_ref(),
            Some(AdminOptions {
                user: choices.admin_user.clone(),
                password: Some(request.password.clone()),
                api_key: request.api_key.clone().filter(|key| !key.is_empty()),
            }),
        )
        .await
        .map_err(failure)?;
        // A retry can follow successful bootstrap and a failed import. Apply the
        // chosen password to that newly-created account only, never to an old instance.
        if database_exists
            && let Some(key) = request.api_key.filter(|key| !key.is_empty())
            && key != desktop.data_plane().gateway_key
        {
            return Err(AppError::Conflict("the gateway key was already created; keep the original key or leave this field empty".into()).into());
        }
        if database_exists {
            let data = desktop.app().data();
            Operations::new(desktop.app().gproxy(), &data, desktop.app().config())
                .users()
                .set_password(&desktop.caller().user_id, &request.password)
                .await?;
            desktop.app().reload_all().await?;
        }
        drop(request.password);
        let import_report = if let Some(import) = request.import {
            let report = desktop
                .app()
                .gproxy()
                .manage()
                .transfer()
                .import(import)
                .await?;
            desktop.app().reload_all().await?;
            Some(report)
        } else {
            None
        };
        // OS startup registration may involve WinRT and must not block the IPC executor.
        let auto_start = choices.auto_start;
        tauri::async_runtime::spawn_blocking(move || crate::startup::set(auto_start))
            .await
            .map_err(failure)?
            .map_err(failure)?;
        #[cfg(desktop)]
        if choices.tray {
            crate::tray::install(app).map_err(failure)?;
        }
        choices.completed = true;
        write_choices(&self.root, &choices).map_err(failure)?;
        let plane = desktop.data_plane();
        let result = SetupResult {
            base_url: plane.base_url.clone(),
            api_key: plane.gateway_key.clone(),
            import_report,
            existing_admin_preserved: !desktop.admin_created() && !database_exists,
        };
        app.manage(desktop);
        Ok(result)
    }
}

pub fn read_choices(root: &Path) -> StartResult<Choices> {
    let file = root.join(FILE);
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mut choices: Choices = serde_json::from_slice(&bytes).map_err(|error| {
                StartError::io(
                    "reading first-run settings",
                    std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                )
            })?;
            choices.database = config::read_file(&choices.data_dir)?.store;
            Ok(choices)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let config = config::read_file(root)?;
            let completed =
                config::database_file(root, &config.store).is_some_and(|path| path.is_file());
            Ok(Choices {
                data_dir: root.to_owned(),
                host: config.host,
                port: config.port,
                admin_user: "admin".to_owned(),
                auto_start: false,
                tray: cfg!(desktop) && !completed,
                completed,
                database: config.store,
            })
        }
        Err(error) => Err(StartError::io("reading first-run settings", error)),
    }
}

fn database_kinds() -> Vec<&'static str> {
    let mut kinds = vec!["sqlite"];
    if cfg!(feature = "postgres") {
        kinds.push("postgres");
    }
    if cfg!(feature = "mysql") {
        kinds.push("mysql");
    }
    kinds
}

fn validate_database(database: &StoreBackendConfig) -> IpcResult<()> {
    match database {
        StoreBackendConfig::Sqlite { path } if !path.trim().is_empty() => {
            #[cfg(any(target_os = "android", target_env = "ohos"))]
            if Path::new(path).is_absolute()
                || Path::new(path)
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(AppError::invalid(
                    "Mobile database files must stay inside the private data directory",
                )
                .into());
            }
            Ok(())
        }
        StoreBackendConfig::Url { dsn }
            if (cfg!(feature = "postgres")
                && (dsn.starts_with("postgres://") || dsn.starts_with("postgresql://")))
                || (cfg!(feature = "mysql") && dsn.starts_with("mysql://")) =>
        {
            Ok(())
        }
        _ => Err(AppError::invalid(
            "choose a supported database and provide its file path or connection URL",
        )
        .into()),
    }
}

fn write_choices(root: &Path, choices: &Choices) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    let bytes = serde_json::to_vec_pretty(choices)?;
    let temporary = root.join("desktop-setup.tmp");
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, root.join(FILE))
}

fn write_config(
    data_dir: &Path,
    host: &str,
    port: u16,
    database: &StoreBackendConfig,
) -> StartResult<()> {
    let path = data_dir.join(config::CONFIG_FILE);
    let mut table = match std::fs::read_to_string(&path) {
        Ok(text) => toml::from_str::<toml::Table>(&text)
            .map_err(|error| StartError::App(AppError::invalid(error.to_string())))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(error) => return Err(StartError::io("reading instance configuration", error)),
    };
    table.insert("host".into(), toml::Value::String(host.into()));
    table.insert("port".into(), toml::Value::Integer(i64::from(port)));
    table.insert(
        "store".into(),
        toml::Value::try_from(database)
            .map_err(|error| StartError::App(AppError::invalid(error.to_string())))?,
    );
    let text = toml::to_string_pretty(&table)
        .map_err(|error| StartError::App(AppError::invalid(error.to_string())))?;
    let temporary = data_dir.join("gproxy.toml.tmp");
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| StartError::io("writing instance configuration", error))?;
    file.write_all(text.as_bytes())
        .map_err(|error| StartError::io("writing instance configuration", error))?;
    drop(file);
    std::fs::rename(temporary, path)
        .map_err(|error| StartError::io("saving instance configuration", error))
}

fn failure(error: impl std::fmt::Display) -> IpcError {
    IpcError {
        code: "setup_failed".into(),
        message: error.to_string(),
        status: 400,
    }
}
