#[derive(Debug, Clone)]
pub enum CountError {
    InvalidTokenizer(String),
    Encode(String),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTokenizer(reason) => write!(f, "invalid tokenizer: {reason}"),
            Self::Encode(reason) => write!(f, "tokenizer encode failed: {reason}"),
        }
    }
}

impl std::error::Error for CountError {}
