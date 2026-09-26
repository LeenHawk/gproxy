//! One instance, two front doors.
//!
//! [`Desktop`] is the value every IPC command and every HTTP request in this
//! process reaches through. It holds **one** [`App`] — and therefore one
//! `Gproxy`, one identity snapshot, one cache and one SQLite connection — and
//! hands the same reference to both surfaces. Two assemblies over one database
//! would be two snapshots publishing independently, two background syncs
//! polling each other's writes, and a credential refreshed in one that the
//! other does not see until its next poll.
//!
//! # What startup does, in order
//!
//! 1. decide whether this is a new instance (the database file is the answer);
//! 2. resolve the master key ([`crate::secrets`]) — before the handle exists,
//!    because the handle's codec is fixed at assembly;
//! 3. build the settings ([`crate::config`]);
//! 4. assemble, through [`gproxy::instance::open`] — the *same* function the
//!    server uses, so the schema sync, the rotation, the cache and the first
//!    snapshot are done once in the workspace and not twice;
//! 5. create the administrator if the instance is new, through
//!    [`gproxy::bootstrap`];
//! 6. make sure there is a gateway key, and keep it;
//! 7. bind the data plane on loopback and serve it.
//!
//! Step 4 is the whole reason this crate is small. `gproxy::instance::open`
//! already knows how to turn an `AppConfig` into a running instance; a desktop
//! shell that reimplemented it would be a second opinion about SQLite pool
//! sizes and rotation ordering, and the two would drift.
//!
//! # The caller on the IPC side
//!
//! The person at the window is the instance administrator, and the IPC channel
//! is the trust boundary that establishes it — a message arrives over it only
//! because this process's own webview sent it. So [`Desktop::caller`] is a
//! [`Caller`] for the local administrator, built once at startup, and no IPC
//! command authenticates anything. The embedded HTTP data plane is a different
//! matter and authenticates every request: see [`crate::dataplane`].

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use gproxy::{bootstrap, instance};
use gproxy_app::{
    App, Caller, CallerKind, Operations,
    dto::{ApiKeyWrite, ListQuery},
};
use serde::Serialize;

use crate::{
    StartError, StartResult, config,
    secrets::{self, Placement, SecretStore},
};

/// The connection a desktop instance always has. Fixed rather than generic
/// because `#[tauri::command]` functions cannot be generic — a command table
/// is a list of concrete function items — and because the desktop uses the same native connection as the CLI.
pub type Connection = instance::Connection;

/// The name of the key the shell keeps in the keychain for the data plane.
const GATEWAY_KEY_NAME: &str = "desktop";

/// One running desktop instance.
///
/// Cheap to clone; every field is an `Arc` or a small value. Tauri holds one
/// in its managed state and hands `&Desktop` to every command.
#[derive(Clone)]
pub struct Desktop {
    app: Arc<App<Connection>>,
    caller: Caller,
    data_plane: DataPlane,
    /// The server's router over this instance, which the console in the
    /// window is answered by. Never bound to a socket; see [`crate::console`].
    management: axum::Router,
    data_dir: PathBuf,
    secrets: SecretPlacement,
    admin_created: bool,
    /// Stops the embedded HTTP server. Cloned into [`Desktop::shutdown`].
    stop: Arc<tokio::sync::Notify>,
}

/// Where the data plane is listening and what it demands.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPlane {
    /// The address that was actually bound, which is not the configured one
    /// when `gproxy.toml` asked for port 0.
    pub address: SocketAddr,
    /// What a client puts in front of `/v1/messages`.
    pub base_url: String,
    /// The gateway key. Handed to the window on request so a person can paste
    /// it into a client's configuration; it never leaves this machine.
    pub gateway_key: String,
}

/// Which store each secret ended up in, for the banner the window shows.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretPlacement {
    pub master_key: Placement,
    pub gateway_key: Placement,
}

impl SecretPlacement {
    /// Whether upstream credentials are sealed. `false` is the fact the window
    /// has to show prominently, and it is the same fact the server logs.
    pub fn secrets_are_sealed(self) -> bool {
        self.master_key.is_keychain()
    }
}

impl Desktop {
    /// Assemble, bootstrap, bind and start serving.
    ///
    /// `store` is the credential store, taken as a parameter so a test can
    /// drive the whole assembly without a keyring daemon, a D-Bus session or a
    /// logged-in desktop. Production passes [`secrets::Keychain`].
    pub async fn start(data_dir: PathBuf, store: &dyn SecretStore) -> StartResult<Self> {
        Self::start_with_admin(data_dir, store, None).await
    }

    pub(crate) async fn start_with_admin(
        data_dir: PathBuf,
        store: &dyn SecretStore,
        admin: Option<gproxy::config::AdminOptions>,
    ) -> StartResult<Self> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| StartError::io(format!("creating {}", data_dir.display()), error))?;

        // Asked before anything is opened: `instance::open` creates the file,
        // and after that the question cannot be answered any more.
        let configured = config::read_file(&data_dir)?;
        let fresh = config::database_file(&data_dir, &configured.store).map_or_else(
            || !data_dir.join("secrets.json").is_file(),
            |path| !path.is_file(),
        );
        let master_key = secrets::master_key(store, &data_dir, fresh)?;
        let master_key_placement = master_key.placement;
        let mut settings = config::settings(&data_dir, master_key)?;
        if let Some(admin) = admin {
            settings.admin = admin;
        }

        let instance = instance::open(&settings, instance::OpenOptions::serving()).await?;
        if !master_key_placement.is_keychain() {
            // The same fact the server states, in the same words, with this
            // host's remedy after it. The window shows it too — see
            // `SecretPlacement` — because a desktop user does not read logs.
            tracing::warn!(
                "{} This machine has no usable system keychain, so there was nowhere to put one. \
                 Upstream logins stored here can be read by anything that can read \
                 {}.",
                gproxy::rotate::PLAINTEXT_SECRETS,
                config::database_path(&data_dir).display()
            );
        }

        let report = bootstrap::ensure_admin(&instance.app, &settings.admin).await?;
        // Not `report.announce()`: that prints a generated password and a
        // minted key to standard output, which is the right thing for somebody
        // who typed a command and the wrong thing for a window. Neither secret
        // is generated here anyway — the password is refused in
        // `config::settings`, and the key goes to the keychain below.
        let admin_created = report.created();
        if admin_created {
            tracing::info!(user = settings.admin.user, "created this instance");
        }

        let (gateway_key, gateway_placement) = if report.created()
            && let Some(token) = settings.admin.api_key.as_ref()
        {
            let placement = secrets::store_gateway_key(store, &data_dir, token)?;
            (token.clone(), placement)
        } else {
            resolve_gateway_key(&instance.app, store, &data_dir, report).await?
        };

        let stop = Arc::new(tokio::sync::Notify::new());
        let address =
            crate::dataplane::serve(instance.app.clone(), &settings.config, Arc::clone(&stop))
                .await?;

        let caller = local_administrator(&instance.app).await?;
        tracing::info!(
            %address,
            revision = instance.app.snapshot().revision(),
            sealed = master_key_placement.is_keychain(),
            "the desktop instance is running"
        );

        Ok(Self {
            management: gproxy_host_axum::router(gproxy_host_axum::HostState::new(
                instance.app.clone(),
            )),
            app: instance.app,
            admin_created,
            caller,
            data_plane: DataPlane {
                address,
                base_url: local_base_url(address),
                gateway_key,
            },
            data_dir,
            secrets: SecretPlacement {
                master_key: master_key_placement,
                gateway_key: gateway_placement,
            },
            stop,
        })
    }

    pub(crate) fn admin_created(&self) -> bool {
        self.admin_created
    }

    pub fn app(&self) -> &Arc<App<Connection>> {
        &self.app
    }

    /// The person at the window, as every `Portal` operation needs them.
    ///
    /// Constructed once rather than resolved per command: there is exactly one
    /// account on a desktop instance and nothing inside the application can
    /// change whose it is. A `Session` caller rather than an `ApiKey` one
    /// because that is what it is — a person, not a token.
    pub fn caller(&self) -> &Caller {
        &self.caller
    }

    pub fn data_plane(&self) -> &DataPlane {
        &self.data_plane
    }

    pub fn management(&self) -> &axum::Router {
        &self.management
    }

    pub fn secrets(&self) -> SecretPlacement {
        self.secrets
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    /// Stop the data plane and the background sync, in that order: the sync
    /// keeps both snapshots fresh for whatever is still in flight.
    pub fn shutdown(&self) {
        self.stop.notify_waiters();
        self.app.shutdown();
    }

    /// Build the request's `Operations` over a snapshot loaded exactly once.
    ///
    /// The snapshot is the caller's local because `Operations` borrows it; the
    /// one-load-per-request rule is the same one both HTTP hosts follow, and
    /// here "request" means "one IPC command".
    pub fn operations<'a>(&'a self, data: &'a gproxy_app::AppData) -> Operations<'a, Connection> {
        Operations::new(self.app.gproxy(), data, self.app.config())
    }
}

impl std::fmt::Debug for Desktop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The gateway key is inside `DataPlane`, so the address is printed and
        // the struct is not.
        f.debug_struct("Desktop")
            .field("address", &self.data_plane.address)
            .field("secrets", &self.secrets)
            .finish_non_exhaustive()
    }
}

/// The key the data plane authenticates with, from wherever it already is.
///
/// Three cases, and the third is the one worth having:
///
/// - the instance was just created — bootstrap minted a key and handed it back
///   exactly once, so it goes into the keychain now or never;
/// - the instance existed and the keychain has the key — use it;
/// - the instance existed and the keychain does not — mint a new one for the
///   administrator. A cleared keyring must not leave a desktop instance whose
///   own data plane it can never call again.
async fn resolve_gateway_key(
    app: &Arc<App<Connection>>,
    store: &dyn SecretStore,
    data_dir: &std::path::Path,
    report: bootstrap::Report,
) -> StartResult<(String, Placement)> {
    if let bootstrap::Report::Created {
        api_key: Some(token),
        ..
    } = &report
    {
        let placement = secrets::store_gateway_key(store, data_dir, token)?;
        return Ok((token.clone(), placement));
    }
    if let Some(token) = secrets::gateway_key(store, data_dir) {
        return Ok((token, Placement::Keychain));
    }

    let user_id = administrator_id(app).await?;
    let data = app.data();
    let created = Operations::new(app.gproxy(), &data, app.config())
        .api_keys()
        .create(ApiKeyWrite {
            id: None,
            user_id,
            name: GATEWAY_KEY_NAME.to_owned(),
            organization_id: None,
            team_id: None,
            expires_at_ms: None,
            enabled: Some(true),
            // The keychain is the copy that matters; the database keeping a
            // second one would be a second place to lose it from.
            retain_secret: Some(false),
            // For local tools calling models. The desktop's own management
            // runs in process as the person at the window, not through a key.
            management: None,
            budget: None,
        })
        .await?;
    // The key has to authenticate on the very next request, not after the next
    // poll, so the snapshot is republished before the socket is bound.
    app.reload_all().await?;
    let placement = secrets::store_gateway_key(store, data_dir, &created.token)?;
    tracing::warn!(
        "this instance had no gateway key where it left one, so a new one was minted. Any client \
         configured with the old key will have to be given the new one."
    );
    Ok((created.token, placement))
}

/// The local administrator, as a `Caller`.
async fn local_administrator(app: &Arc<App<Connection>>) -> StartResult<Caller> {
    Ok(Caller {
        user_id: administrator_id(app).await?,
        user_role: "admin".to_owned(),
        api_key_id: None,
        organization_id: None,
        team_id: None,
        grant: None,
        kind: CallerKind::Session,
        management: false,
    })
}

/// The id of the administrator this shell runs as.
///
/// Read through the `users` family rather than with a query of its own: it is
/// the same family every `admin_users_*` command calls, so there is one
/// opinion in this process about what a user row looks like. By name first,
/// because that is the account `bootstrap` created and the one the window is;
/// falling back to any administrator covers a database restored from a server
/// instance, where the account is called something else.
async fn administrator_id(app: &Arc<App<Connection>>) -> StartResult<String> {
    let data = app.data();
    let operations = Operations::new(app.gproxy(), &data, app.config());
    let named = operations
        .users()
        .list(ListQuery {
            search: Some(config::DESKTOP_ADMIN_USER.to_owned()),
            ..ListQuery::default()
        })
        .await?;
    if let Some(user) = named
        .items
        .into_iter()
        .find(|user| user.name == config::DESKTOP_ADMIN_USER)
    {
        return Ok(user.id);
    }
    operations
        .users()
        .list(ListQuery::default())
        .await?
        .items
        .into_iter()
        .find(|user| user.role == "admin")
        .map(|user| user.id)
        .ok_or_else(|| {
            StartError::App(gproxy_app::AppError::internal(
                "this instance has no administrator account",
            ))
        })
}

/// Unspecified listeners are reachable through loopback on this device.
fn local_base_url(mut address: std::net::SocketAddr) -> String {
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    format!("http://{address}")
}
