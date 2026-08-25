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

#[cfg(test)]
mod tests;
