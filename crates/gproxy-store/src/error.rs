#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Amount(#[from] gproxy_seaorm::FixedDecimalError),
    #[error("invalid store operation: {0}")]
    Invalid(String),
    #[error("unexpected batch result")]
    UnexpectedResult,
    #[error("cannot create operation receipt: {0}")]
    Entropy(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub(crate) fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Invalid(message.into())
}

pub(crate) fn receipt() -> Result<Vec<u8>> {
    let mut bytes = vec![0; 32];
    getrandom::fill(&mut bytes).map_err(|e| StoreError::Entropy(e.to_string()))?;
    Ok(bytes)
}
