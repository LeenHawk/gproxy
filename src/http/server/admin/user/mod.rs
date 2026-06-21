//! §F7a portal API (`/user/*`) — native-only, behind `require_session`.
//! Every read/write scopes to the session user's id; request-supplied ids are ignored.

pub mod account;
pub mod audit;
pub mod authz;
pub mod keys;
pub mod me;
pub mod usage;
