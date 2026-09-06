use crate::backend::Row;
use crate::query::oauth;
use crate::records::{
    CredentialEnvelope, OAuthAccessIdentity, OAuthCodeInput, OAuthCodeRecord, OAuthDeviceInput,
    OAuthDeviceRecord, OAuthGrantInput, OAuthGrantRecord, OAuthTokenInput, OAuthTokenRecord,
};
use crate::{Store, StoreError};

impl Store {
    pub async fn cancel_oauth_device(&self, id: i64, now: i64) -> Result<(), StoreError> {
        self.backend().batch(oauth::cancel_device(id, now)?).await?;
        Ok(())
    }

    pub async fn create_oauth_authorization(
        &self,
        input: &crate::records::OAuthAuthorizationInput,
    ) -> Result<bool, StoreError> {
        let results = self
            .backend()
            .batch(crate::query::oauth_authorization::create(input)?)
            .await?;
        Ok(results[3].affected_rows == 1)
    }

    pub async fn start_oauth_device(&self, input: &OAuthDeviceInput) -> Result<bool, StoreError> {
        let results = self
            .backend()
            .batch(crate::query::oauth_authorization::device(input)?)
            .await?;
        Ok(results[1].affected_rows == 1)
    }

    pub async fn insert_oauth_grant(&self, input: &OAuthGrantInput) -> Result<i64, StoreError> {
        self.insert(oauth::insert_grant(input)?).await
    }

    pub async fn insert_oauth_code(&self, input: &OAuthCodeInput) -> Result<i64, StoreError> {
        self.insert(oauth::insert_code(input)?).await
    }

    pub async fn oauth_code(&self, digest: &[u8]) -> Result<Option<OAuthCodeRecord>, StoreError> {
        self.backend()
            .execute(oauth::code(digest)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_code)
            .transpose()
    }

    pub async fn consume_oauth_code(&self, id: i64, now: i64) -> Result<bool, StoreError> {
        self.update(oauth::consume_code(id, now)?).await
    }

    pub async fn insert_oauth_token(&self, input: &OAuthTokenInput) -> Result<i64, StoreError> {
        self.insert(oauth::insert_token(input)?).await
    }

    pub async fn oauth_token(&self, digest: &[u8]) -> Result<Option<OAuthTokenRecord>, StoreError> {
        self.backend()
            .execute(oauth::token(digest)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_token)
            .transpose()
    }

    pub async fn consume_oauth_token(&self, id: i64, now: i64) -> Result<bool, StoreError> {
        self.update(oauth::consume_token(id, now)?).await
    }

    pub async fn revoke_oauth_grant(
        &self,
        id: i64,
        user_key_id: i64,
        now: i64,
    ) -> Result<(), StoreError> {
        self.backend()
            .batch(vec![
                oauth::revoke_grant(id, now)?,
                oauth::revoke_tokens(id, now)?,
                oauth::disable_user_key(user_key_id)?,
            ])
            .await?;
        Ok(())
    }

    pub async fn oauth_access_identity(
        &self,
        digest: &[u8],
        now: i64,
    ) -> Result<Option<OAuthAccessIdentity>, StoreError> {
        self.backend()
            .execute(oauth::access_identity(digest, now)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_access_identity)
            .transpose()
    }

    pub async fn insert_oauth_device(&self, input: &OAuthDeviceInput) -> Result<i64, StoreError> {
        self.insert(oauth::insert_device(input)?).await
    }

    pub async fn oauth_device_by_digest(
        &self,
        digest: &[u8],
    ) -> Result<Option<OAuthDeviceRecord>, StoreError> {
        self.backend()
            .execute(oauth::device_by_digest(digest)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_device)
            .transpose()
    }

    pub async fn oauth_device_by_code(
        &self,
        code: &str,
    ) -> Result<Option<OAuthDeviceRecord>, StoreError> {
        self.backend()
            .execute(oauth::device_by_code(code)?)
            .await?
            .rows
            .into_iter()
            .next()
            .map(parse_device)
            .transpose()
    }

    pub async fn approve_oauth_device(
        &self,
        id: i64,
        grant_id: i64,
        envelope: &CredentialEnvelope,
        now: i64,
    ) -> Result<bool, StoreError> {
        self.update(oauth::approve_device(id, grant_id, envelope, now)?)
            .await
    }
}

fn parse_grant(row: &Row) -> Result<OAuthGrantRecord, StoreError> {
    Ok(OAuthGrantRecord {
        id: row.i64("id")?,
        user_id: row.i64("user_id")?,
        user_key_id: row.i64("user_key_id")?,
        provider_id: row.optional_i64("provider_id")?,
        client_id: row.text("client_id")?.to_owned(),
        scopes: row.text("scopes")?.to_owned(),
        chatgpt_user_id: row.text("chatgpt_user_id")?.to_owned(),
        chatgpt_account_id: row.text("chatgpt_account_id")?.to_owned(),
        revoked_at: row.optional_i64("revoked_at")?,
    })
}

fn parse_code(row: Row) -> Result<OAuthCodeRecord, StoreError> {
    Ok(OAuthCodeRecord {
        id: row.i64("code_id")?,
        grant: parse_grant(&row)?,
        redirect_uri: row.text("redirect_uri")?.to_owned(),
        code_challenge: row.text("code_challenge")?.to_owned(),
        expires_at: row.i64("expires_at")?,
        consumed_at: row.optional_i64("consumed_at")?,
    })
}

fn parse_token(row: Row) -> Result<OAuthTokenRecord, StoreError> {
    Ok(OAuthTokenRecord {
        id: row.i64("token_id")?,
        grant: parse_grant(&row)?,
        kind: row.text("kind")?.to_owned(),
        expires_at: row.i64("expires_at")?,
        consumed_at: row.optional_i64("consumed_at")?,
        revoked_at: row.optional_i64("revoked_at")?,
    })
}

fn parse_access_identity(row: Row) -> Result<OAuthAccessIdentity, StoreError> {
    Ok(OAuthAccessIdentity {
        user_id: row.i64("user_id")?,
        user_key_id: row.i64("user_key_id")?,
        organization_id: row.optional_i64("organization_id")?,
        team_id: row.optional_i64("team_id")?,
        expires_at: row.i64("expires_at")?,
        scopes: row.text("scopes")?.into(),
        client_id: row.text("client_id")?.into(),
    })
}

fn parse_device(row: Row) -> Result<OAuthDeviceRecord, StoreError> {
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
                field: "oauth device envelope",
                message: "envelope columns are only partially populated".into(),
            });
        }
    };
    Ok(OAuthDeviceRecord {
        id: row.i64("id")?,
        user_code: row.text("user_code")?.to_owned(),
        client_id: row.text("client_id")?.to_owned(),
        provider_id: row.optional_i64("provider_id")?,
        expires_at: row.i64("expires_at")?,
        grant_id: row.optional_i64("grant_id")?,
        approved_at: row.optional_i64("approved_at")?,
        consumed_at: row.optional_i64("consumed_at")?,
        denied_at: row.optional_i64("denied_at")?,
        envelope,
    })
}

impl Store {
    pub async fn exchange_oauth_tokens(
        &self,
        source: crate::records::OAuthExchangeSource,
        client_id: &str,
        access: &OAuthTokenInput,
        refresh: &OAuthTokenInput,
    ) -> Result<bool, StoreError> {
        let results = self
            .backend()
            .batch(crate::query::oauth_exchange::exchange(
                source, client_id, access, refresh,
            )?)
            .await?;
        Ok(results[2].affected_rows == 1)
    }

    pub async fn deny_oauth_device(&self, id: i64, now: i64) -> Result<bool, StoreError> {
        self.update(oauth::deny_device(id, now)?).await
    }
}
