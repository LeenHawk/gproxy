#[derive(Debug)]
pub enum CountError {
    InvalidJson(serde_json::Error),
    InvalidRequest(&'static str),
    InvalidTokenizer(String),
    Encode(String),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(error) => write!(f, "invalid request JSON: {error}"),
            Self::InvalidRequest(reason) => write!(f, "invalid request: {reason}"),
            Self::InvalidTokenizer(reason) => write!(f, "invalid tokenizer: {reason}"),
            Self::Encode(reason) => write!(f, "tokenizer encode failed: {reason}"),
        }
    }
}

impl std::error::Error for CountError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidJson(error) => Some(error),
            _ => None,
        }
    }
}
