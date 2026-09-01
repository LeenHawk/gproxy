#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerVocabRecord {
    pub name: String,
    pub repository: String,
    pub size_bytes: u64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerVocabData {
    pub repository: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TokenizerAuthSecret {
    pub kind: String,
    pub envelope: super::CredentialEnvelope,
}

impl std::fmt::Debug for TokenizerAuthSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenizerAuthSecret")
            .field("kind", &self.kind)
            .field("envelope", &"<redacted>")
            .finish()
    }
}
