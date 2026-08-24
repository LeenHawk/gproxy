use crate::backend::Row;
use crate::query::binding;
use crate::records::{BindingInput, BindingPage, BindingRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn save_binding(&self, input: &BindingInput) -> Result<(), StoreError> {
        self.backend()
            .execute(binding::upsert_binding(input, unix_now()?)?)
            .await?;
        Ok(())
    }

    pub async fn find_binding(
        &self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &str,
        resource_id: &str,
    ) -> Result<Option<BindingRecord>, StoreError> {
        let result = self
            .backend()
            .execute(binding::find_binding(
                provider_id,
                owner_user_id,
                kind,
                resource_id,
            )?)
            .await?;
        result.rows.into_iter().next().map(parse).transpose()
    }

    pub async fn delete_binding(
        &self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &str,
        resource_id: &str,
    ) -> Result<(), StoreError> {
        self.backend()
            .execute(binding::delete_binding(
                provider_id,
                owner_user_id,
                kind,
                resource_id,
            )?)
            .await?;
        Ok(())
    }

    pub async fn list_bindings(
        &self,
        provider_id: i64,
        owner_user_id: i64,
        kind: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<BindingPage, StoreError> {
        if limit == 0 {
            return Ok(BindingPage {
                items: Vec::new(),
                next_cursor: None,
            });
        }
        let result = self
            .backend()
            .execute(binding::list_bindings(
                provider_id,
                owner_user_id,
                kind,
                cursor,
                limit,
            )?)
            .await?;
        let mut items = result
            .rows
            .into_iter()
            .map(parse)
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = has_more
            .then(|| items.last().map(|item| item.resource_id.clone()))
            .flatten();
        Ok(BindingPage { items, next_cursor })
    }
}

fn parse(row: Row) -> Result<BindingRecord, StoreError> {
    Ok(BindingRecord {
        provider_id: row.i64("provider_id")?,
        owner_user_id: row.i64("owner_user_id")?,
        kind: row.text("kind")?.to_owned(),
        resource_id: row.text("resource_id")?.to_owned(),
        credential_id: row.i64("credential_id")?,
        summary: serde_json::from_str(row.text("summary_json")?).map_err(|error| {
            StoreError::InvalidData {
                field: "summary_json",
                message: error.to_string(),
            }
        })?,
        created_at: row.i64("created_at")?,
    })
}

fn unix_now() -> Result<i64, StoreError> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| StoreError::Database(error.to_string()))?
        .as_secs();
    i64::try_from(seconds).map_err(|error| StoreError::Database(error.to_string()))
}
