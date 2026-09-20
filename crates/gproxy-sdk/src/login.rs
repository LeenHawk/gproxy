//! Turning a person's browser session into a credential row.
//!
//! Authorization code with PKCE, device code and cookie exchange, each through
//! the channel's own login trait. The pending session lives in the shared
//! cache, so the instance that finishes a login need not be the one that
//! started it. Filled in by the login phase.
