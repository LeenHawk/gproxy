use crate::backend::Row;
use crate::query::{admin, admin_auth, admin_seed};
use crate::records::{
    AdminUserRecord, AuditEventInput, AuditEventRecord, CredentialEnvelope, UserAuthRecord,
    UserKeySecretRecord, UserSessionInput,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn has_admin_users(&self) -> Result<bool, StoreError> {
        Ok(!self
            .backend()
            .execute(admin_seed::has_admin_users()?)
            .await?
            .rows
            .is_empty())
    }

    pub async fn create_first_admin(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<Option<i64>, StoreError> {
        let mut results = self
            .backend()
            .batch(vec![
                admin_seed::ensure_default_organization()?,
                admin_seed::promote_first_admin(username, password_hash)?,
                admin_seed::insert_first_admin(username, password_hash)?,
                admin_seed::ensure_admin_permission(username)?,
                admin_auth::admin_by_username(username)?,
            ])
            .await?;
        let account = results
            .pop()
            .expect("admin creation query result")
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()?;
        let changed = results[1].affected_rows > 0 || results[2].affected_rows > 0;
        Ok(changed.then(|| account.map(|value| value.id)).flatten())
    }

    /// Set an existing administrator's password. Returns whether a row
    /// matched. Command line and environment are authoritative: an operator
    /// who passes `--admin-password` is instructing, not suggesting.
    pub async fn set_admin_password(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<bool, StoreError> {
        let result = self
            .backend()
            .execute(admin_auth::set_admin_password(username, password_hash)?)
            .await?;
        Ok(result.affected_rows > 0)
    }

    pub async fn set_user_password(
        &self,
        id: i64,
        password_hash: &str,
    ) -> Result<bool, StoreError> {
        let result = self
            .backend()
            .execute(admin_auth::set_user_password(id, password_hash)?)
            .await?;
        Ok(result.affected_rows > 0)
    }

    pub async fn user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserAuthRecord>, StoreError> {
        self.backend()
            .execute(admin_auth::user_by_username(username)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_user_auth)
            .transpose()
    }

    pub async fn admin_by_username(
        &self,
        username: &str,
    ) -> Result<Option<AdminUserRecord>, StoreError> {
        self.backend()
            .execute(admin_auth::admin_by_username(username)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()
    }

    pub async fn create_user_session(&self, input: &UserSessionInput) -> Result<i64, StoreError> {
        self.insert(admin_auth::insert_user_session(input)?).await
    }

    pub async fn admin_for_api_key(
        &self,
        digest: &[u8],
        now: i64,
    ) -> Result<Option<AdminUserRecord>, StoreError> {
        self.backend()
            .execute(admin_auth::admin_for_api_key(digest, now)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()
    }

    pub async fn admin_for_session(
        &self,
        token_digest: &[u8],
        now: i64,
    ) -> Result<Option<AdminUserRecord>, StoreError> {
        self.backend()
            .execute(admin_auth::admin_for_session(token_digest, now)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_admin)
            .transpose()
    }

    pub async fn user_for_session(
        &self,
        token_digest: &[u8],
        now: i64,
    ) -> Result<Option<UserAuthRecord>, StoreError> {
        self.backend()
            .execute(admin_auth::user_for_session(token_digest, now)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_user_auth)
            .transpose()
    }

    pub async fn delete_user_session(&self, token_digest: &[u8]) -> Result<(), StoreError> {
        self.backend()
            .execute(admin_auth::delete_user_session(token_digest)?)
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

fn parse_admin(row: Row) -> Result<AdminUserRecord, StoreError> {
    Ok(AdminUserRecord {
        id: row.i64("id")?,
        name: row.text("name")?.to_owned(),
        password_hash: row.text("password_hash")?.to_owned(),
        enabled: row.i64("enabled")? != 0,
    })
}

fn parse_user_auth(row: Row) -> Result<UserAuthRecord, StoreError> {
    Ok(UserAuthRecord {
        id: row.i64("id")?,
        name: row.text("name")?.to_owned(),
        organization_id: row.optional_i64("organization_id")?,
        team_id: row.optional_i64("team_id")?,
        password_hash: row.text("password_hash")?.to_owned(),
        enabled: row.i64("enabled")? != 0,
    })
}

fn parse_audit(row: Row) -> Result<AuditEventRecord, StoreError> {
    Ok(AuditEventRecord {
        id: row.i64("id")?,
        event: AuditEventInput {
            actor_user_id: row.i64("actor_user_id")?,
            action: row.text("action")?.to_owned(),
            target_kind: row.text("target_kind")?.to_owned(),
            target_id: row.optional_i64("target_id")?,
            at: row.i64("at")?,
            client_ip: row.optional_text("client_ip")?.map(str::to_owned),
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
