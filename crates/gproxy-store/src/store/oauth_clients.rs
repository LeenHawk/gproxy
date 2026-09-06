use crate::backend::Row;
use crate::query::oauth_clients;
use crate::records::{OAuthClientInput, OAuthClientRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn oauth_clients(&self) -> Result<Vec<OAuthClientRecord>, StoreError> {
        self.backend()
            .execute(oauth_clients::list(None)?)
            .await?
            .rows
            .into_iter()
            .map(parse)
            .collect()
    }

    pub async fn oauth_client(
        &self,
        client_id: &str,
    ) -> Result<Option<OAuthClientRecord>, StoreError> {
        self.backend()
            .execute(oauth_clients::list(Some(client_id))?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse)
            .transpose()
    }

    pub async fn insert_oauth_client(&self, input: &OAuthClientInput) -> Result<i64, StoreError> {
        self.insert(oauth_clients::create(input)?).await
    }

    pub async fn update_oauth_client(
        &self,
        id: i64,
        input: &OAuthClientInput,
        deleted_at: Option<i64>,
        now: i64,
    ) -> Result<(), StoreError> {
        let mut statements = vec![oauth_clients::update(id, input, deleted_at)?];
        if !input.enabled || deleted_at.is_some() {
            statements.extend(oauth_clients::revoke_sessions(&input.client_id, now)?);
        }
        self.backend().batch(statements).await?;
        Ok(())
    }
}

fn parse(row: Row) -> Result<OAuthClientRecord, StoreError> {
    Ok(OAuthClientRecord {
        id: row.i64("id")?,
        client_id: row.text("client_id")?.into(),
        name: row.text("name")?.into(),
        redirect_uris: serde_json::from_str(row.text("redirect_uris")?).map_err(|error| {
            StoreError::InvalidData {
                field: "redirect_uris",
                message: error.to_string(),
            }
        })?,
        enabled: row.i64("enabled")? != 0,
        deleted_at: row.optional_i64("deleted_at")?,
    })
}
