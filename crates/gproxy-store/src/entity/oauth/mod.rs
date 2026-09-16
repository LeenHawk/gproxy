//! GProxy-issued OAuth authorization for downstream clients, not upstream login.
//! Public clients use authorization-code + PKCE S256 or device authorization.
//! Grants bind a user and an internal API key; authorization still intersects
//! current user permissions. Token exchange/rotation/revocation require atomic
//! writes in the future store layer; entity fields alone do not implement them.

pub mod client;
pub mod code;
pub mod device;
pub mod grant;
pub mod token;
