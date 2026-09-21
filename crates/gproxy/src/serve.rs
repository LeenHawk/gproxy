//! Binding a socket and serving the router until a signal arrives.
//!
//! # What startup does, in order
//!
//! 1. assemble the instance ([`crate::instance::open`]), which rotates the
//!    master key if that was asked for and loads the first snapshot;
//! 2. warn about anything the operator would want to know before traffic
//!    arrives — plaintext secrets, a rotation that now needs promoting, a
//!    console that is not built, a cache that cannot be shared;
//! 3. create the first administrator if the instance is new;
//! 4. bind;
//! 5. serve, holding the connection info the client-address logic needs.
//!
//! **The bind comes after the database work**, not before. v3 reserved the port
//! first so a second instance failed fast, but that means the kernel accepts
//! connections into a backlog nothing is answering yet while migrations run. A
//! socket that is not yet bound refuses immediately, which is what a load
//! balancer wants to see from a process that is not ready.
//!
//! # Shutdown
//!
//! `SIGINT` or `SIGTERM` stops accepting and lets in-flight work finish: axum's
//! graceful shutdown drains the requests, and the sdk's background sync is
//! cancelled after they are done. There is no timeout, deliberately — a
//! streaming completion legitimately runs for minutes, and a supervisor that
//! wants a deadline has `TimeoutStopSec` and `SIGKILL`.

use std::net::SocketAddr;

use gproxy_host_axum::{HostState, router};
use tokio::net::TcpListener;

use crate::{Error, Result, Settings, bootstrap, instance, rotate};

/// Assemble, bootstrap, bind and serve until a signal arrives.
pub async fn run(settings: Settings) -> Result<()> {
    let instance = instance::open(&settings, instance::OpenOptions::serving()).await?;
    warn_about(&settings, &instance);

    let report = bootstrap::ensure_admin(&instance.app, &settings.admin).await?;
    report.announce();

    let address = listen_address(&settings)?;
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| Error::io(format!("binding {address}"), error))?;
    // The address that was actually bound, which is not the configured one when
    // the operator asked for port 0.
    let bound = listener
        .local_addr()
        .map_err(|error| Error::io("reading the bound address", error))?;

    let state = HostState::new(instance.app.clone());
    let console = state.console().is_enabled();
    tracing::info!(
        address = %bound,
        // Read now, not from `instance`: bootstrap may have committed a
        // revision since, and this line is what an operator compares against
        // `/healthz`.
        revision = instance.app.snapshot().revision(),
        console,
        "gproxy is listening"
    );

    let service = router(state)
        // `ConnectInfo` is how the client address reaches admission. Without it
        // the host treats every peer as unknown — and unknown is deliberately
        // not trusted, so `x-forwarded-for` would be ignored and every request
        // would share one rate-limit bucket.
        .into_make_service_with_connect_info::<SocketAddr>();
    let result = axum::serve(listener, service)
        .with_graceful_shutdown(signal())
        .await;

    tracing::info!("draining finished; shutting down");
    // After the requests, not before: the background sync keeps the snapshot
    // fresh for whatever is still in flight.
    instance.app.gproxy().shutdown();
    result.map_err(|error| Error::io("serving", error))
}

/// Everything worth saying once, at startup, rather than per request.
fn warn_about(settings: &Settings, instance: &instance::Instance) {
    if instance.secrets.is_plaintext() {
        // Loud, and it names the variable. An operator who reads this line and
        // does nothing has made a decision; one who never sees it has not.
        // The first sentence is `rotate::PLAINTEXT_SECRETS`, shared with the
        // desktop shell; only the remedy is this host's.
        tracing::warn!(
            "{} Set GPROXY_MASTER_KEY to 32 bytes as 64 hex characters or base64 — and, on a \
             database that already holds secrets, set GPROXY_MASTER_KEY_NEXT with \
             GPROXY_MASTER_KEY_ROTATE to seal what is already there.",
            rotate::PLAINTEXT_SECRETS
        );
    }
    if let Some(rotated) = instance.secrets.rotated {
        tracing::warn!(
            secrets = rotated.total(),
            "this process is now using the NEW master key; promote GPROXY_MASTER_KEY_NEXT to \
             GPROXY_MASTER_KEY before restarting, or the next start will not open anything"
        );
    }
    if matches!(
        settings.config.cache,
        gproxy_app::config::CacheBackendConfig::Memory
    ) {
        // Not a warning: one instance is the normal deployment. It is a
        // debug-level note so that an operator debugging why two instances
        // disagree finds the reason in their own logs.
        tracing::debug!(
            "the cache is process-local; a second instance over this database needs Redis, or the \
             two will not see each other's writes or share rate limits"
        );
    }
    if settings.config.console.enabled && !instance_console_available(settings) {
        tracing::info!(
            "no console bundle is compiled into this binary and no directory was named, so \
             /console answers 404. Build `console/` and rebuild, or point GPROXY_CONSOLE_PATH at a \
             development build."
        );
    }
}

/// Whether a console *could* be served: a directory was named, or the binary
/// was built with a bundle in it.
///
/// The bundle lives in `gproxy-host-axum/assets/web`, filled by the console
/// build before `cargo build`. A source checkout embeds nothing, and
/// [`gproxy_host_axum::console::Console::from_config`] resolves to `Disabled` —
/// which is why this asks the host rather than guessing.
fn instance_console_available(settings: &Settings) -> bool {
    settings.config.console.path.is_some()
        || gproxy_host_axum::console::Console::from_config(&settings.config.console).is_enabled()
}

fn listen_address(settings: &Settings) -> Result<SocketAddr> {
    let host = settings.config.host.trim();
    let port = settings.config.port;
    // Parsed as an address pair rather than resolved as a name: a listen address
    // that needs DNS is a configuration mistake, and resolving one could bind an
    // interface the operator did not name.
    format!("{host}:{port}")
        .parse()
        .or_else(|_| format!("[{host}]:{port}").parse())
        .map_err(|_| {
            Error::config(
                "--host / GPROXY_HOST",
                format!("`{host}` is not an IP address to listen on"),
            )
        })
}

/// `SIGINT` or, on Unix, `SIGTERM`.
///
/// `SIGTERM` is the one that matters: it is what a container runtime, systemd
/// and Kubernetes send, and a process that ignores it is a process that gets
/// `SIGKILL`ed mid-stream a few seconds later.
async fn signal() {
    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                // Unreachable in practice. Never returning is the right failure:
                // the alternative is resolving immediately and shutting the
                // server down the moment it started.
                Err(error) => {
                    tracing::error!(%error, "cannot listen for SIGTERM; only ctrl-c will stop this process");
                    std::future::pending::<()>().await;
                }
            }
        };
        tokio::select! {
            _ = interrupt() => {}
            _ = terminate => {}
        }
    }
    #[cfg(not(unix))]
    interrupt().await;
}

async fn interrupt() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "cannot listen for ctrl-c");
        std::future::pending::<()>().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_app::AppConfig;

    fn settings(host: &str, port: u16) -> Settings {
        Settings {
            config: AppConfig {
                host: host.into(),
                port,
                ..AppConfig::default()
            },
            admin: Default::default(),
            telemetry: Default::default(),
            instance_id: None,
        }
    }

    #[test]
    fn an_ipv4_and_an_ipv6_host_both_bind() {
        assert_eq!(
            listen_address(&settings("127.0.0.1", 7070)).unwrap(),
            "127.0.0.1:7070".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            listen_address(&settings("0.0.0.0", 80)).unwrap(),
            "0.0.0.0:80".parse::<SocketAddr>().unwrap()
        );
        // Bare and bracketed IPv6 are the same address.
        for host in ["::1", "[::1]"] {
            assert_eq!(
                listen_address(&settings(host, 7070)).unwrap(),
                "[::1]:7070".parse::<SocketAddr>().unwrap(),
                "{host}"
            );
        }
    }

    #[test]
    fn a_hostname_is_refused_rather_than_resolved() {
        let error = listen_address(&settings("localhost", 7070)).unwrap_err();
        assert!(
            error.to_string().contains("--host / GPROXY_HOST"),
            "{error}"
        );
    }
}
