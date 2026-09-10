#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("credential secret malformed: {0}")]
    Secret(String),
    #[error("request preparation failed: {0}")]
    Prepare(String),
    #[error("refresh failed: {0}")]
    Refresh(String),
    #[error("response observation failed: {0}")]
    Observe(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("login failed: {0}")]
    Login(String),
    #[error("unsupported channel operation: {0}")]
    Unsupported(&'static str),
}
