use crate::query::tokenizer;
use crate::{Store, StoreError};

impl Store {
    pub async fn tokenizer_vocab_names(&self) -> Result<Vec<String>, StoreError> {
        self.backend()
            .execute(tokenizer::list()?)
            .await?
            .rows
            .into_iter()
            .map(|row| row.text("name").map(str::to_owned))
            .collect()
    }

    pub async fn tokenizer_vocab(&self, name: &str) -> Result<Option<Vec<u8>>, StoreError> {
        let mut result = self.backend().execute(tokenizer::get(name)?).await?;
        let Some(row) = result.rows.pop() else {
            return Ok(None);
        };
        Ok(Some(row.blob("bytes")?.to_vec()))
    }

    pub async fn put_tokenizer_vocab(&self, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
        self.backend()
            .execute(tokenizer::put(name, bytes, unix_now())?)
            .await?;
        Ok(())
    }

    pub async fn delete_tokenizer_vocab(&self, name: &str) -> Result<(), StoreError> {
        self.backend().execute(tokenizer::delete(name)?).await?;
        Ok(())
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is before unix epoch")
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
