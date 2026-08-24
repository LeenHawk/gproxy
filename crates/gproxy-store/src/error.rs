#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Database(String),
    #[error("invalid stored {field}: {message}")]
    InvalidData {
        field: &'static str,
        message: String,
    },
    #[error("credential version conflict")]
    VersionConflict,
}
