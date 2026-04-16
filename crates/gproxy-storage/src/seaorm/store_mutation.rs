use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::*;
use time::OffsetDateTime;

use gproxy_core::api_key_digest;

use crate::seaorm::SeaOrmStorage;
use crate::seaorm::entities::*;
use crate::write::UsageWrite;

impl SeaOrmStorage {
    pub async fn create_provider(
        &self,
        name: &str,
        channel: &str,
        settings_json: &str,
        routing_json: &str,
    ) -> Result<i64, DbErr> {
        let settings: serde_json::Value = serde_json::from_str(settings_json)
            .map_err(|e| DbErr::Custom(format!("invalid settings_json: {e}")))?;
        let routing: serde_json::Value = serde_json::from_str(routing_json)
            .map_err(|e| DbErr::Custom(format!("invalid routing_json: {e}")))?;
        let now = OffsetDateTime::now_utc();
        let model = providers::ActiveModel {
            name: Set(name.to_string()),
            channel: Set(channel.to_string()),
            settings_json: Set(settings),
            routing_json: Set(routing),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = providers::Entity::insert(model).exec(&self.db).await?;
        Ok(result.last_insert_id)
    }

    pub async fn create_credential(
        &self,
        provider_id: i64,
        name: Option<&str>,
        kind: &str,
        secret_json: &str,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let secret: serde_json::Value = serde_json::from_str(secret_json)
            .map_err(|e| DbErr::Custom(format!("invalid secret_json: {e}")))?;
        let encrypted_secret = self.encrypt_json(&secret);
        let now = OffsetDateTime::now_utc();
        let model = credentials::ActiveModel {
            provider_id: Set(provider_id),
            name: Set(name.map(String::from)),
            kind: Set(kind.to_string()),
            secret_json: Set(encrypted_secret),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = credentials::Entity::insert(model).exec(&self.db).await?;
        Ok(result.last_insert_id)
    }

    pub async fn create_user(
        &self,
        name: &str,
        password: &str,
        enabled: bool,
        is_admin: bool,
    ) -> Result<i64, DbErr> {
        let encrypted_password = self.encrypt_string(password);
        let now = OffsetDateTime::now_utc();
        let model = users::ActiveModel {
            name: Set(name.to_string()),
            password: Set(Some(encrypted_password)),
            enabled: Set(enabled),
            is_admin: Set(is_admin),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = users::Entity::insert(model).exec(&self.db).await?;
        Ok(result.last_insert_id)
    }

    pub async fn create_user_key(
        &self,
        user_id: i64,
        api_key: &str,
        label: Option<&str>,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let encrypted_key = self.encrypt_string(api_key);
        let api_key_digest = api_key_digest(api_key);
        let now = OffsetDateTime::now_utc();
        let model = user_keys::ActiveModel {
            user_id: Set(user_id),
            api_key_ciphertext: Set(encrypted_key),
            api_key_digest: Set(api_key_digest),
            label: Set(label.map(String::from)),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = user_keys::Entity::insert(model).exec(&self.db).await?;
        Ok(result.last_insert_id)
    }

    pub async fn add_user_quota_cost(&self, user_id: i64, cost: f64) -> Result<(f64, f64), DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = user_token_usage::ActiveModel {
            id: NotSet,
            user_id: Set(user_id),
            quota: Set(0.0),
            cost_used: Set(cost),
            updated_at: Set(now),
        };
        user_token_usage::Entity::insert(model)
            .on_conflict(
                OnConflict::column(user_token_usage::Column::UserId)
                    .value(
                        user_token_usage::Column::CostUsed,
                        Expr::col(user_token_usage::Column::CostUsed).add(cost),
                    )
                    .value(user_token_usage::Column::UpdatedAt, Expr::value(now))
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        let row = user_token_usage::Entity::find()
            .filter(user_token_usage::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or_else(|| DbErr::Custom("quota row missing after cost increment".to_string()))?;
        Ok((row.quota, row.cost_used))
    }

    pub async fn record_usage_and_quota_cost(
        &self,
        usage: UsageWrite,
        cost: f64,
    ) -> Result<Option<(f64, f64)>, DbErr> {
        let txn = self.db.begin().await?;

        let quota_state = if cost > 0.0 {
            let now = OffsetDateTime::now_utc();
            let model = user_token_usage::ActiveModel {
                id: NotSet,
                user_id: Set(usage.user_id.unwrap_or_default()),
                quota: Set(0.0),
                cost_used: Set(cost),
                updated_at: Set(now),
            };
            user_token_usage::Entity::insert(model)
                .on_conflict(
                    OnConflict::column(user_token_usage::Column::UserId)
                        .value(
                            user_token_usage::Column::CostUsed,
                            Expr::col(user_token_usage::Column::CostUsed).add(cost),
                        )
                        .value(user_token_usage::Column::UpdatedAt, Expr::value(now))
                        .to_owned(),
                )
                .exec(&txn)
                .await?;

            let row = user_token_usage::Entity::find()
                .filter(user_token_usage::Column::UserId.eq(usage.user_id.unwrap_or_default()))
                .one(&txn)
                .await?
                .ok_or_else(|| {
                    DbErr::Custom("quota row missing after cost increment".to_string())
                })?;
            Some((row.quota, row.cost_used))
        } else {
            None
        };

        usages::Entity::insert(usages::ActiveModel {
            trace_id: NotSet,
            downstream_trace_id: Set(usage.downstream_trace_id),
            at: Set(unix_ms_to_datetime(usage.at_unix_ms)),
            provider_id: Set(usage.provider_id),
            credential_id: Set(usage.credential_id),
            user_id: Set(usage.user_id),
            user_key_id: Set(usage.user_key_id),
            operation: Set(usage.operation),
            protocol: Set(usage.protocol),
            model: Set(usage.model),
            input_tokens: Set(usage.input_tokens),
            output_tokens: Set(usage.output_tokens),
            cache_read_input_tokens: Set(usage.cache_read_input_tokens),
            cache_creation_input_tokens: Set(usage.cache_creation_input_tokens),
            cache_creation_input_tokens_5min: Set(usage.cache_creation_input_tokens_5min),
            cache_creation_input_tokens_1h: Set(usage.cache_creation_input_tokens_1h),
            cost: Set(cost),
            created_at: Set(OffsetDateTime::now_utc()),
        })
        .exec(&txn)
        .await?;

        txn.commit().await?;
        Ok(quota_state)
    }

    pub async fn create_model(
        &self,
        provider_id: i64,
        model_id: &str,
        display_name: Option<&str>,
        enabled: bool,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = models::ActiveModel {
            provider_id: Set(provider_id),
            model_id: Set(model_id.to_string()),
            display_name: Set(display_name.map(String::from)),
            enabled: Set(enabled),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = models::Entity::insert(model).exec(&self.db).await?;
        Ok(result.last_insert_id)
    }

    pub async fn create_user_model_permission(
        &self,
        user_id: i64,
        provider_id: Option<i64>,
        model_pattern: &str,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = user_model_permissions::ActiveModel {
            user_id: Set(user_id),
            provider_id: Set(provider_id),
            model_pattern: Set(model_pattern.to_string()),
            created_at: Set(now),
            ..Default::default()
        };
        let result = user_model_permissions::Entity::insert(model)
            .exec(&self.db)
            .await?;
        Ok(result.last_insert_id)
    }

    pub async fn create_user_rate_limit(
        &self,
        user_id: i64,
        model_pattern: &str,
        rpm: Option<i32>,
        rpd: Option<i32>,
        total_tokens: Option<i64>,
    ) -> Result<i64, DbErr> {
        let now = OffsetDateTime::now_utc();
        let model = user_rate_limits::ActiveModel {
            user_id: Set(user_id),
            model_pattern: Set(model_pattern.to_string()),
            rpm: Set(rpm),
            rpd: Set(rpd),
            total_tokens: Set(total_tokens),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let result = user_rate_limits::Entity::insert(model)
            .exec(&self.db)
            .await?;
        Ok(result.last_insert_id)
    }

    pub async fn clear_upstream_request_payloads(
        &self,
        trace_ids: Option<&[i64]>,
    ) -> Result<u64, DbErr> {
        let mut update = upstream_requests::Entity::update_many()
            .col_expr(
                upstream_requests::Column::RequestBody,
                Expr::value(Option::<Vec<u8>>::None),
            )
            .col_expr(
                upstream_requests::Column::ResponseBody,
                Expr::value(Option::<Vec<u8>>::None),
            );
        if let Some(ids) = trace_ids {
            update = update.filter(upstream_requests::Column::TraceId.is_in(ids.to_vec()));
        }
        let result = update.exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    pub async fn clear_downstream_request_payloads(
        &self,
        trace_ids: Option<&[i64]>,
    ) -> Result<u64, DbErr> {
        let mut update = downstream_requests::Entity::update_many()
            .col_expr(
                downstream_requests::Column::RequestBody,
                Expr::value(Option::<Vec<u8>>::None),
            )
            .col_expr(
                downstream_requests::Column::ResponseBody,
                Expr::value(Option::<Vec<u8>>::None),
            );
        if let Some(ids) = trace_ids {
            update = update.filter(downstream_requests::Column::TraceId.is_in(ids.to_vec()));
        }
        let result = update.exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    pub async fn delete_upstream_requests(&self, trace_ids: Option<&[i64]>) -> Result<u64, DbErr> {
        let mut delete = upstream_requests::Entity::delete_many();
        if let Some(ids) = trace_ids {
            delete = delete.filter(upstream_requests::Column::TraceId.is_in(ids.to_vec()));
        }
        let result = delete.exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    pub async fn delete_downstream_requests(
        &self,
        trace_ids: Option<&[i64]>,
    ) -> Result<u64, DbErr> {
        let mut delete = downstream_requests::Entity::delete_many();
        if let Some(ids) = trace_ids {
            delete = delete.filter(downstream_requests::Column::TraceId.is_in(ids.to_vec()));
        }
        let result = delete.exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    pub async fn delete_usages(&self, trace_ids: Option<&[i64]>) -> Result<u64, DbErr> {
        let mut delete = usages::Entity::delete_many();
        if let Some(ids) = trace_ids {
            delete = delete.filter(usages::Column::TraceId.is_in(ids.to_vec()));
        }
        let result = delete.exec(&self.db).await?;
        Ok(result.rows_affected)
    }

    // --- Encryption helpers (write direction) ---

    fn encrypt_string(&self, plaintext: &str) -> String {
        match &self.cipher {
            Some(cipher) => cipher
                .encrypt_string(plaintext)
                .unwrap_or_else(|_| plaintext.to_string()),
            None => plaintext.to_string(),
        }
    }

    fn encrypt_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match &self.cipher {
            Some(cipher) => cipher.encrypt_json(value).unwrap_or_else(|_| value.clone()),
            None => value.clone(),
        }
    }
}

fn unix_ms_to_datetime(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::SeaOrmStorage;

    #[tokio::test]
    async fn create_user_key_rejects_duplicate_plaintext_keys_when_encryption_is_enabled() {
        let storage = SeaOrmStorage::connect("sqlite::memory:", Some("test-secret"))
            .await
            .expect("connect storage");
        storage.sync().await.expect("sync schema");

        let user_id_1 = storage
            .create_user("alice", "password", true, false)
            .await
            .expect("create first user");
        let user_id_2 = storage
            .create_user("bob", "password", true, false)
            .await
            .expect("create second user");

        storage
            .create_user_key(user_id_1, "sk-duplicate", Some("first"), true)
            .await
            .expect("create first key");

        let err = storage
            .create_user_key(user_id_2, "sk-duplicate", Some("second"), true)
            .await
            .expect_err("duplicate plaintext key must be rejected");

        let message = err.to_string().to_lowercase();
        assert!(
            message.contains("unique") || message.contains("duplicate"),
            "unexpected duplicate-key error: {message}"
        );
    }
}
