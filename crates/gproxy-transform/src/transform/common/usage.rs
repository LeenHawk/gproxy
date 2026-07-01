#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_creation_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

impl TokenUsage {
    pub const fn empty() -> Self {
        Self {
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            reasoning_tokens: None,
        }
    }

    pub fn total_tokens(self) -> Option<u64> {
        checked_sum([
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cache_creation_tokens,
        ])
    }
}

pub fn checked_sum(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    values
        .into_iter()
        .flatten()
        .try_fold(0_u64, |acc, value| acc.checked_add(value))
}
