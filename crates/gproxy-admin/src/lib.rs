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

#[cfg(test)]
mod tests;
