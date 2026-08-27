#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration environment: {0}")]
    Environment(String),
    #[error("invalid config field `{field}`: {message}")]
    Invalid {
        field: &'static str,
        message: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Store(#[from] gproxy_store::StoreError),
    #[error("initialize core: {0}")]
    Core(#[from] gproxy_core::InitError),
    #[error("channel registry: {0}")]
    Channels(#[from] gproxy_channel_api::registry::DuplicateChannel),
    #[error("secret encryption: {0}")]
    Encryption(String),
    #[error("cache: {0}")]
    Cache(String),
    #[error("control write: {0}")]
    Control(String),
    #[error("bootstrap: {0}")]
    Bootstrap(String),
}
