//! Framework-free control-plane dispatch shared by every gproxy host.

mod auth;
mod dispatch;
pub mod dto;
mod error;
mod handlers;
mod portal;
mod response;
mod route;
mod state;

pub use auth::AuthSource;
pub use dispatch::dispatch;
pub use error::AdminError;
pub use portal::{PortalIdentity, dispatch as portal_dispatch};
pub use state::State;

pub async fn seed_first_admin(
    store: &gproxy_store::Store,
    username: &str,
    password: &str,
) -> Result<Option<i64>, AdminError> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AdminError::BadRequest("username must not be blank".into()));
    }
    auth::password::validate(password)?;
    let hash = auth::password::hash(password)?;
    store
        .create_first_admin(username, &hash, auth::now()?)
        .await
        .map_err(Into::into)
}

/// Apply the administrator password the operator supplied on the command
/// line or in the environment. Creates the account when the store has none,
/// otherwise sets the password on the named one. Those inputs are
/// authoritative: silently ignoring an explicit `--admin-password` because a
/// row already exists is how an operator ends up locked out of their own
/// instance with nothing in the log to explain it.
pub async fn apply_admin_password(
    store: &gproxy_store::Store,
    username: &str,
    password: &str,
) -> Result<i64, AdminError> {
    let username = username.trim();
    if username.is_empty() {
        return Err(AdminError::BadRequest("username must not be blank".into()));
    }
    auth::password::validate(password)?;
    let hash = auth::password::hash(password)?;
    if let Some(id) = store
        .create_first_admin(username, &hash, auth::now()?)
        .await?
    {
        return Ok(id);
    }
    if store.set_admin_password(username, &hash).await? {
        return store
            .admin_by_username(username)
            .await?
            .map(|account| account.id)
            .ok_or_else(|| AdminError::Internal("administrator vanished".into()));
    }
    Err(AdminError::BadRequest(format!(
        "no administrator named `{username}`; the store already has a different account"
    )))
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn authorize_host_route(
    state: &impl State,
    parts: &http::request::Parts,
    write: bool,
) -> Result<(), http::Response<bytes::Bytes>> {
    let result = async {
        let _admin = auth::authenticate(state, parts).await?;
        if write {
            auth::verify_same_origin(parts)?;
        }
        Ok::<_, AdminError>(())
    }
    .await;
    result.map_err(|error| response::render(Err(error), "admin"))
}

#[cfg(test)]
mod tests;
