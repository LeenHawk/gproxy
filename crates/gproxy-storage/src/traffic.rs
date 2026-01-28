use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue, Database, DatabaseConnection, DbErr, Schema};
use time::OffsetDateTime;

use crate::entities;

#[derive(Debug, Clone)]
pub struct DownstreamTrafficEvent {
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub user_id: Option<i64>,
    pub key_id: Option<i64>,
    pub request_id: Option<String>,

    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,

    pub response_status: i32,
    pub response_headers: String,
    pub response_body: String,

    pub claude_input_tokens: Option<i64>,
    pub claude_output_tokens: Option<i64>,
    pub claude_total_tokens: Option<i64>,
    pub claude_cache_creation_input_tokens: Option<i64>,
    pub claude_cache_read_input_tokens: Option<i64>,

    pub gemini_prompt_tokens: Option<i64>,
    pub gemini_candidates_tokens: Option<i64>,
    pub gemini_total_tokens: Option<i64>,
    pub gemini_cached_tokens: Option<i64>,

    pub openai_chat_prompt_tokens: Option<i64>,
    pub openai_chat_completion_tokens: Option<i64>,
    pub openai_chat_total_tokens: Option<i64>,

    pub openai_responses_input_tokens: Option<i64>,
    pub openai_responses_output_tokens: Option<i64>,
    pub openai_responses_total_tokens: Option<i64>,
    pub openai_responses_input_cached_tokens: Option<i64>,
    pub openai_responses_output_reasoning_tokens: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UpstreamTrafficEvent {
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub credential_id: Option<i64>,
    pub request_id: Option<String>,

    pub request_method: String,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_headers: String,
    pub request_body: String,

    pub response_status: i32,
    pub response_headers: String,
    pub response_body: String,

    pub claude_input_tokens: Option<i64>,
    pub claude_output_tokens: Option<i64>,
    pub claude_total_tokens: Option<i64>,
    pub claude_cache_creation_input_tokens: Option<i64>,
    pub claude_cache_read_input_tokens: Option<i64>,

    pub gemini_prompt_tokens: Option<i64>,
    pub gemini_candidates_tokens: Option<i64>,
    pub gemini_total_tokens: Option<i64>,
    pub gemini_cached_tokens: Option<i64>,

    pub openai_chat_prompt_tokens: Option<i64>,
    pub openai_chat_completion_tokens: Option<i64>,
    pub openai_chat_total_tokens: Option<i64>,

    pub openai_responses_input_tokens: Option<i64>,
    pub openai_responses_output_tokens: Option<i64>,
    pub openai_responses_total_tokens: Option<i64>,
    pub openai_responses_input_cached_tokens: Option<i64>,
    pub openai_responses_output_reasoning_tokens: Option<i64>,
}

#[derive(Clone)]
pub struct TrafficStorage {
    db: DatabaseConnection,
}

impl TrafficStorage {
    pub async fn connect(database_url: &str) -> Result<Self, DbErr> {
        let db = Database::connect(database_url).await?;
        Ok(Self { db })
    }

    pub async fn from_connection(db: DatabaseConnection) -> Result<Self, DbErr> {
        Ok(Self { db })
    }

    pub fn connection(&self) -> &DatabaseConnection {
        &self.db
    }

    pub async fn sync(&self) -> Result<(), DbErr> {
        Schema::new(self.db.get_database_backend())
            .builder()
            .register(entities::Users)
            .register(entities::ApiKeys)
            .register(entities::Providers)
            .register(entities::Credentials)
            .register(entities::CredentialDisallow)
            .register(entities::GlobalConfig)
            .register(entities::DownstreamTraffic)
            .register(entities::UpstreamTraffic)
            .sync(&self.db)
            .await
    }

    pub async fn insert_downstream(
        &self,
        event: DownstreamTrafficEvent,
    ) -> Result<(), DbErr> {
        let now = OffsetDateTime::now_utc();
        let mut active: entities::downstream_traffic::ActiveModel = event.into();
        active.created_at = ActiveValue::Set(now);
        entities::DownstreamTraffic::insert(active)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn insert_upstream(&self, event: UpstreamTrafficEvent) -> Result<(), DbErr> {
        let now = OffsetDateTime::now_utc();
        let mut active: entities::upstream_traffic::ActiveModel = event.into();
        active.created_at = ActiveValue::Set(now);
        entities::UpstreamTraffic::insert(active)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn upsert_global_config(
        &self,
        id: i64,
        config_json: Json,
        updated_at: OffsetDateTime,
    ) -> Result<(), DbErr> {
        use entities::global_config::Column;

        let active = entities::global_config::ActiveModel {
            id: ActiveValue::Set(id),
            config_json: ActiveValue::Set(config_json),
            updated_at: ActiveValue::Set(updated_at),
            ..Default::default()
        };

        entities::GlobalConfig::insert(active)
            .on_conflict(
                OnConflict::column(Column::Id)
                    .update_columns([Column::ConfigJson, Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn ensure_admin_user(&self, admin_key: &str) -> Result<(), DbErr> {
        let now = OffsetDateTime::now_utc();

        let user_active = entities::users::ActiveModel {
            id: ActiveValue::Set(0),
            name: ActiveValue::Set(Some("admin".to_string())),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };

        entities::Users::insert(user_active)
            .on_conflict(
                OnConflict::column(entities::users::Column::Id)
                    .update_columns([
                        entities::users::Column::Name,
                        entities::users::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        let key_active = entities::api_keys::ActiveModel {
            id: ActiveValue::Set(0),
            user_id: ActiveValue::Set(0),
            key_value: ActiveValue::Set(admin_key.to_string()),
            label: ActiveValue::Set(Some("admin".to_string())),
            enabled: ActiveValue::Set(true),
            created_at: ActiveValue::Set(now),
            last_used_at: ActiveValue::Set(None),
            ..Default::default()
        };

        entities::ApiKeys::insert(key_active)
            .on_conflict(
                OnConflict::column(entities::api_keys::Column::Id)
                    .update_columns([
                        entities::api_keys::Column::UserId,
                        entities::api_keys::Column::KeyValue,
                        entities::api_keys::Column::Label,
                        entities::api_keys::Column::Enabled,
                        entities::api_keys::Column::LastUsedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        Ok(())
    }
}

impl From<DownstreamTrafficEvent> for entities::downstream_traffic::ActiveModel {
    fn from(event: DownstreamTrafficEvent) -> Self {
        entities::downstream_traffic::ActiveModel {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::NotSet,
            provider: ActiveValue::Set(event.provider),
            provider_id: ActiveValue::Set(event.provider_id),
            operation: ActiveValue::Set(event.operation),
            model: ActiveValue::Set(event.model),
            user_id: ActiveValue::Set(event.user_id),
            key_id: ActiveValue::Set(event.key_id),
            request_id: ActiveValue::Set(event.request_id),
            request_method: ActiveValue::Set(event.request_method),
            request_path: ActiveValue::Set(event.request_path),
            request_query: ActiveValue::Set(event.request_query),
            request_headers: ActiveValue::Set(event.request_headers),
            request_body: ActiveValue::Set(event.request_body),
            response_status: ActiveValue::Set(event.response_status),
            response_headers: ActiveValue::Set(event.response_headers),
            response_body: ActiveValue::Set(event.response_body),
            claude_input_tokens: ActiveValue::Set(event.claude_input_tokens),
            claude_output_tokens: ActiveValue::Set(event.claude_output_tokens),
            claude_total_tokens: ActiveValue::Set(event.claude_total_tokens),
            claude_cache_creation_input_tokens: ActiveValue::Set(
                event.claude_cache_creation_input_tokens,
            ),
            claude_cache_read_input_tokens: ActiveValue::Set(event.claude_cache_read_input_tokens),
            gemini_prompt_tokens: ActiveValue::Set(event.gemini_prompt_tokens),
            gemini_candidates_tokens: ActiveValue::Set(event.gemini_candidates_tokens),
            gemini_total_tokens: ActiveValue::Set(event.gemini_total_tokens),
            gemini_cached_tokens: ActiveValue::Set(event.gemini_cached_tokens),
            openai_chat_prompt_tokens: ActiveValue::Set(event.openai_chat_prompt_tokens),
            openai_chat_completion_tokens: ActiveValue::Set(
                event.openai_chat_completion_tokens,
            ),
            openai_chat_total_tokens: ActiveValue::Set(event.openai_chat_total_tokens),
            openai_responses_input_tokens: ActiveValue::Set(event.openai_responses_input_tokens),
            openai_responses_output_tokens: ActiveValue::Set(event.openai_responses_output_tokens),
            openai_responses_total_tokens: ActiveValue::Set(event.openai_responses_total_tokens),
            openai_responses_input_cached_tokens: ActiveValue::Set(
                event.openai_responses_input_cached_tokens,
            ),
            openai_responses_output_reasoning_tokens: ActiveValue::Set(
                event.openai_responses_output_reasoning_tokens,
            ),
        }
    }
}

impl From<UpstreamTrafficEvent> for entities::upstream_traffic::ActiveModel {
    fn from(event: UpstreamTrafficEvent) -> Self {
        entities::upstream_traffic::ActiveModel {
            id: ActiveValue::NotSet,
            created_at: ActiveValue::NotSet,
            provider: ActiveValue::Set(event.provider),
            provider_id: ActiveValue::Set(event.provider_id),
            operation: ActiveValue::Set(event.operation),
            model: ActiveValue::Set(event.model),
            credential_id: ActiveValue::Set(event.credential_id),
            request_id: ActiveValue::Set(event.request_id),
            request_method: ActiveValue::Set(event.request_method),
            request_path: ActiveValue::Set(event.request_path),
            request_query: ActiveValue::Set(event.request_query),
            request_headers: ActiveValue::Set(event.request_headers),
            request_body: ActiveValue::Set(event.request_body),
            response_status: ActiveValue::Set(event.response_status),
            response_headers: ActiveValue::Set(event.response_headers),
            response_body: ActiveValue::Set(event.response_body),
            claude_input_tokens: ActiveValue::Set(event.claude_input_tokens),
            claude_output_tokens: ActiveValue::Set(event.claude_output_tokens),
            claude_total_tokens: ActiveValue::Set(event.claude_total_tokens),
            claude_cache_creation_input_tokens: ActiveValue::Set(
                event.claude_cache_creation_input_tokens,
            ),
            claude_cache_read_input_tokens: ActiveValue::Set(event.claude_cache_read_input_tokens),
            gemini_prompt_tokens: ActiveValue::Set(event.gemini_prompt_tokens),
            gemini_candidates_tokens: ActiveValue::Set(event.gemini_candidates_tokens),
            gemini_total_tokens: ActiveValue::Set(event.gemini_total_tokens),
            gemini_cached_tokens: ActiveValue::Set(event.gemini_cached_tokens),
            openai_chat_prompt_tokens: ActiveValue::Set(event.openai_chat_prompt_tokens),
            openai_chat_completion_tokens: ActiveValue::Set(
                event.openai_chat_completion_tokens,
            ),
            openai_chat_total_tokens: ActiveValue::Set(event.openai_chat_total_tokens),
            openai_responses_input_tokens: ActiveValue::Set(event.openai_responses_input_tokens),
            openai_responses_output_tokens: ActiveValue::Set(event.openai_responses_output_tokens),
            openai_responses_total_tokens: ActiveValue::Set(event.openai_responses_total_tokens),
            openai_responses_input_cached_tokens: ActiveValue::Set(
                event.openai_responses_input_cached_tokens,
            ),
            openai_responses_output_reasoning_tokens: ActiveValue::Set(
                event.openai_responses_output_reasoning_tokens,
            ),
        }
    }
}
