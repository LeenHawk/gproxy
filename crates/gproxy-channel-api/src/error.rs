//! Errors returned by channel adapters.

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("missing setting: {0}")]
    MissingSetting(&'static str),
    #[error("invalid credential: {0}")]
    InvalidCredential(String),
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    #[error("build error: {0}")]
    Build(String),
    #[error("transient channel error: {0}")]
    Transient(String),
}
