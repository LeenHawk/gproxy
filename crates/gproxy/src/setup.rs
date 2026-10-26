//! The steps of a first run that every host takes the same way.
//!
//! The desktop wizard and the terminal wizard ask the same questions in the
//! same order — connection, administrator, channels, credentials — and only
//! the asking differs. What they do with the answers lives here, so a rule
//! about a valid port or a provider's default name cannot mean one thing in a
//! window and another in a terminal.

use std::{net::IpAddr, sync::Arc};

use gproxy_app::{App, AppError, Result, config::StoreBackendConfig};
use gproxy_sdk::{
    ChannelDescriptor, RefreshMode,
    dto::{BatchItem, CredentialWrite, ProviderDto, ProviderWrite},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;

/// A trimmed, non-blank administrator name.
pub fn admin_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("administrator name must not be blank"));
    }
    Ok(name.to_owned())
}

/// The password policy the instance enforces at sign-in, checked before
/// anything is written.
pub fn password(password: &str) -> Result<()> {
    gproxy_app::auth::password::validate(password)
}

/// A listening address: an IP literal, not a host name, and a non-zero port.
pub fn listen(host: &str, port: u16) -> Result<IpAddr> {
    let host = host
        .trim()
        .parse()
        .map_err(|_| AppError::invalid("enter a valid listening IP address"))?;
    if port == 0 {
        return Err(AppError::invalid("choose a port between 1 and 65535"));
    }
    Ok(host)
}

/// Fail before creating a database when something already owns the port.
pub fn probe(host: IpAddr, port: u16) -> std::result::Result<(), String> {
    std::net::TcpListener::bind((host, port))
        .map(drop)
        .map_err(|error| format!("cannot listen on {host}:{port}: {error}"))
}

/// The database kinds this build can open, SQLite first.
pub fn database_kinds() -> Vec<&'static str> {
    let mut kinds = vec!["sqlite"];
    if cfg!(feature = "postgres") {
        kinds.push("postgres");
    }
    if cfg!(feature = "mysql") {
        kinds.push("mysql");
    }
    kinds
}

/// A SQLite file name, or a connection URL whose driver this build carries.
pub fn database(database: &StoreBackendConfig) -> Result<()> {
    match database {
        StoreBackendConfig::Sqlite { path } if !path.trim().is_empty() => Ok(()),
        StoreBackendConfig::Url { dsn }
            if (cfg!(feature = "postgres")
                && (dsn.starts_with("postgres://") || dsn.starts_with("postgresql://")))
                || (cfg!(feature = "mysql") && dsn.starts_with("mysql://")) =>
        {
            Ok(())
        }
        _ => Err(AppError::invalid(
            "choose a supported database and provide its file path or connection URL",
        )),
    }
}

/// Every channel this process can serve, ordered by display name: the ones
/// compiled in, with any a downstream binary added replacing those that share
/// an id. Available before an instance exists, because the wizard asks before
/// it opens one.
pub fn channels() -> Vec<ChannelDescriptor> {
    let mut out: Vec<ChannelDescriptor> = Vec::new();
    let extra = crate::EXTRA_CHANNELS.get().into_iter().flatten().cloned();
    for channel in gproxy_sdk::channels::compiled_in().into_iter().chain(extra) {
        let descriptor = channel.descriptor();
        out.retain(|known| known.id != descriptor.id);
        out.push(descriptor);
    }
    out.sort_by(|a, b| a.display_name.cmp(b.display_name));
    out
}

/// The route prefix a new provider of `channel` gets: its id, lowercased,
/// without underscores. The console's provider form offers the same name.
pub fn default_provider_name(channel: &str) -> String {
    channel.to_ascii_lowercase().replace('_', "")
}

/// One provider per channel, with the channel's defaults, in one commit; then
/// a reload, so a login started next finds them in the live snapshot.
pub async fn create_providers<C>(app: &Arc<App<C>>, channels: &[String]) -> Result<Vec<ProviderDto>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if channels.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = self::channels();
    let items = channels
        .iter()
        .map(|id| {
            let descriptor = catalog
                .iter()
                .find(|descriptor| descriptor.id == id.as_str())
                .ok_or_else(|| {
                    AppError::invalid(format!("`{id}` is not a channel this build carries"))
                })?;
            Ok(BatchItem::Create(ProviderWrite {
                name: default_provider_name(id),
                display_name: Some(descriptor.display_name.to_owned()),
                channel: id.clone(),
                enabled: Some(true),
                ..Default::default()
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let created = app.gproxy().manage().providers().batch(items).await?;
    app.reload_all().await?;
    Ok(created.into_iter().flatten().collect())
}

/// An API key as an operator pastes it, as the secret the channel reads: a
/// JSON string or object is taken as written, anything else must be one
/// whitespace-free token and becomes `{"api_key": …}`. The console's
/// credential form applies the same rule.
pub fn api_key_secret(text: &str) -> Result<Value> {
    let text = text.trim();
    match serde_json::from_str::<Value>(text) {
        Ok(Value::String(key)) => api_key_secret(&key),
        Ok(object @ Value::Object(_)) => Ok(object),
        _ if text.is_empty() || text.chars().any(char::is_whitespace) => {
            Err(AppError::invalid("paste one API key without spaces"))
        }
        _ => Ok(serde_json::json!({ "api_key": text })),
    }
}

/// Store an API key on `provider` and run the first refresh a channel may need
/// before the key is usable.
pub async fn add_api_key<C>(app: &Arc<App<C>>, provider: &ProviderDto, text: &str) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let credentials = app.gproxy().manage().credentials();
    let row = credentials
        .create(CredentialWrite {
            provider_id: provider.id.clone(),
            auth_kind: "api_key".to_owned(),
            secret: api_key_secret(text)?,
            enabled: Some(true),
            ..Default::default()
        })
        .await?;
    if row.expires_at_ms == Some(0) {
        credentials.refresh(&row.id, RefreshMode::IfNeeded).await?;
    }
    app.reload_all().await?;
    Ok(())
}
