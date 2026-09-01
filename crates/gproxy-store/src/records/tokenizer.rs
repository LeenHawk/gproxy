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
