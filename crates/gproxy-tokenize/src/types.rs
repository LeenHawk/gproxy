#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMethod {
    Tiktoken,
    HuggingFace,
    BundledFallback,
    CharacterEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountWarning {
    ApproximateProviderFraming { tokens_per_message: u64 },
    GenericJsonHarvest,
    InvalidJson { reason: String },
    RawBodyEstimate,
    TokenizerLoadScheduled { vocab: String },
    TokenizerLoadInFlight { vocab: String },
    TokenizerNegativeCached { vocab: String },
    TokioRuntimeUnavailable { vocab: String },
    TokenizerEncodeFailed { vocab: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountResult {
    pub tokens: u64,
    pub method: CountMethod,
    pub vocab: Option<String>,
    pub warnings: Vec<CountWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CountError {
    InvalidJson(String),
    TokenizerUnavailable(String),
    TokenizerEncodeFailed(String),
    Registry(String),
}

impl std::fmt::Display for CountError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(reason) => write!(formatter, "invalid request JSON: {reason}"),
            Self::TokenizerUnavailable(vocab) => {
                write!(formatter, "tokenizer `{vocab}` is unavailable")
            }
            Self::TokenizerEncodeFailed(vocab) => {
                write!(formatter, "tokenizer `{vocab}` failed to encode input")
            }
            Self::Registry(reason) => write!(formatter, "tokenizer registry failed: {reason}"),
        }
    }
}

impl std::error::Error for CountError {}
