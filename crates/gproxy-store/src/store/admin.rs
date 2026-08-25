use crate::backend::Row;
use crate::query::admin;
use crate::records::{
    AdminAccountRecord, AdminSessionInput, AuditEventInput, AuditEventRecord, CredentialEnvelope,
    UserKeySecretRecord,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn has_admin_accounts(&self) -> Result<bool, StoreError> {
        Ok(!self
            .backend()
            .execute(admin::has_admin_accounts()?)
            .await?
            .rows
            .is_empty())
    }

    pub async fn create_first_admin(
        &self,
        username: &str,
        password_hash: &str,
        created_at: i64,
    ) -> Result<Option<i64>, StoreError> {
        let result = self
            .backend()
            .execute(admin::create_first_admin(
                username,
                password_hash,
                created_at,
            )?)
            .await?;
        Ok((result.affected_rows == 1)
            .then_some(result.last_insert_id)
            .flatten())
    }

    pub async fn admin_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AdminAccountRecord>, StoreError> {
        self.backend()
            .execute(admin::admin_by_username(username)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()
    }

    pub async fn create_admin_session(&self, input: &AdminSessionInput) -> Result<i64, StoreError> {
        self.insert(admin::insert_admin_session(input)?).await
    }

    pub async fn admin_for_session(
        &self,
        token_digest: &[u8],
        now: i64,
    ) -> Result<Option<AdminAccountRecord>, StoreError> {
        self.backend()
            .execute(admin::admin_for_session(token_digest, now)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()
    }

    pub async fn delete_admin_session(&self, token_digest: &[u8]) -> Result<(), StoreError> {
        self.backend()
            .execute(admin::delete_admin_session(token_digest)?)
            .await?;
        Ok(())
    }

    pub async fn record_audit_event(&self, input: &AuditEventInput) -> Result<i64, StoreError> {
        self.insert(admin::insert_audit_event(input)?).await
    }

    pub async fn audit_events(&self, limit: u64) -> Result<Vec<AuditEventRecord>, StoreError> {
        self.backend()
            .execute(admin::select_audit_events(limit)?)
            .await?
            .rows
            .into_iter()
            .map(parse_audit)
            .collect()
    }

    pub async fn user_key_secret(
        &self,
        id: i64,
    ) -> Result<Option<UserKeySecretRecord>, StoreError> {
        self.backend()
            .execute(admin::select_user_key_secret(id)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_user_key_secret)
            .transpose()
    }
}

fn parse_admin(row: Row) -> Result<AdminAccountRecord, StoreError> {
    Ok(AdminAccountRecord {
        id: row.i64("id")?,
        username: row.text("username")?.to_owned(),
        password_hash: row.text("password_hash")?.to_owned(),
        enabled: row.i64("enabled")? != 0,
        created_at: row.i64("created_at")?,
    })
}

fn parse_audit(row: Row) -> Result<AuditEventRecord, StoreError> {
    Ok(AuditEventRecord {
        id: row.i64("id")?,
        event: AuditEventInput {
            actor_admin_id: row.i64("actor_admin_id")?,
            action: row.text("action")?.to_owned(),
            target_kind: row.text("target_kind")?.to_owned(),
            target_id: row.optional_i64("target_id")?,
            at: row.i64("at")?,
            details: row
                .optional_text("details_json")?
                .map(|value| serde_json::from_str(value).map_err(invalid_json))
                .transpose()?,
        },
    })
}

fn parse_user_key_secret(row: Row) -> Result<UserKeySecretRecord, StoreError> {
    let parts = [
        row.optional_blob("ciphertext")?,
        row.optional_blob("wrapped_key")?,
        row.optional_blob("payload_nonce")?,
        row.optional_blob("key_nonce")?,
    ];
    let envelope = match parts {
        [None, None, None, None] => None,
        [
            Some(ciphertext),
            Some(wrapped_key),
            Some(payload_nonce),
            Some(key_nonce),
        ] => Some(CredentialEnvelope {
            ciphertext: ciphertext.to_vec(),
            wrapped_key: wrapped_key.to_vec(),
            payload_nonce: payload_nonce.to_vec(),
            key_nonce: key_nonce.to_vec(),
        }),
        _ => {
            return Err(StoreError::InvalidData {
                field: "user key envelope",
                message: "envelope columns are only partially populated".into(),
            });
        }
    };
    Ok(UserKeySecretRecord {
        id: row.i64("id")?,
        envelope,
    })
}

fn invalid_json(error: serde_json::Error) -> StoreError {
    StoreError::InvalidData {
        field: "audit details",
        message: error.to_string(),
    }
}
