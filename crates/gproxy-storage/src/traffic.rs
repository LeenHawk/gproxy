#![allow(clippy::needless_update)]

use sea_orm::entity::prelude::*;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Schema,
};
use sea_orm::ExprTrait;
use time::OffsetDateTime;

use crate::entities;
use crate::db::connect_shared;
pub use gproxy_provider_core::{DownstreamTrafficEvent, UpstreamTrafficEvent};


#[derive(Debug, Clone)]
pub struct AdminProviderInput {
    pub id: Option<i64>,
    pub name: String,
    pub config_json: Json,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct AdminCredentialInput {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub name: Option<String>,
    pub secret: Json,
    pub meta_json: Json,
    pub weight: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct AdminDisallowInput {
    pub credential_id: i64,
    pub scope_kind: String,
    pub scope_value: Option<String>,
    pub level: String,
    pub until_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminUserInput {
    pub id: Option<i64>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AdminKeyInput {
    pub id: Option<i64>,
    pub user_id: i64,
    pub key_value: String,
    pub label: Option<String>,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct TrafficStorage {
    db: DatabaseConnection,
}

#[derive(Debug, Clone, Default, FromQueryResult)]
pub struct UpstreamUsageAggregate {
    pub count: Option<i64>,
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

impl TrafficStorage {
    pub async fn connect(database_url: &str) -> Result<Self, DbErr> {
        let db = connect_shared(database_url).await?;
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

    pub async fn ensure_providers(
        &self,
        defaults: &[AdminProviderInput],
    ) -> Result<(), DbErr> {
        let existing = self.list_providers().await?;
        let mut existing_names = std::collections::HashSet::new();
        for provider in existing {
            existing_names.insert(provider.name);
        }

        for default in defaults {
            if existing_names.contains(&default.name) {
                continue;
            }
            let mut input = default.clone();
            input.id = None;
            let _ = self.upsert_provider(input).await?;
        }

        Ok(())
    }

    pub async fn health(&self) -> Result<(), DbErr> {
        entities::GlobalConfig::find()
            .order_by_asc(entities::global_config::Column::Id)
            .one(&self.db)
            .await?;
        Ok(())
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

    pub async fn get_upstream_usage(
        &self,
        credential_id: i64,
        model: Option<&str>,
        start_at: OffsetDateTime,
        end_at: OffsetDateTime,
    ) -> Result<UpstreamUsageAggregate, DbErr> {
        use entities::upstream_traffic::Column;

        let mut query = entities::UpstreamTraffic::find().select_only();
        query = query
            .column_as(Expr::col(Column::Id).count(), "count")
            .column_as(
                Expr::col(Column::ClaudeInputTokens).sum(),
                "claude_input_tokens",
            )
            .column_as(
                Expr::col(Column::ClaudeOutputTokens).sum(),
                "claude_output_tokens",
            )
            .column_as(
                Expr::col(Column::ClaudeTotalTokens).sum(),
                "claude_total_tokens",
            )
            .column_as(
                Expr::col(Column::ClaudeCacheCreationInputTokens).sum(),
                "claude_cache_creation_input_tokens",
            )
            .column_as(
                Expr::col(Column::ClaudeCacheReadInputTokens).sum(),
                "claude_cache_read_input_tokens",
            )
            .column_as(
                Expr::col(Column::GeminiPromptTokens).sum(),
                "gemini_prompt_tokens",
            )
            .column_as(
                Expr::col(Column::GeminiCandidatesTokens).sum(),
                "gemini_candidates_tokens",
            )
            .column_as(
                Expr::col(Column::GeminiTotalTokens).sum(),
                "gemini_total_tokens",
            )
            .column_as(
                Expr::col(Column::GeminiCachedTokens).sum(),
                "gemini_cached_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiChatPromptTokens).sum(),
                "openai_chat_prompt_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiChatCompletionTokens).sum(),
                "openai_chat_completion_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiChatTotalTokens).sum(),
                "openai_chat_total_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiResponsesInputTokens).sum(),
                "openai_responses_input_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiResponsesOutputTokens).sum(),
                "openai_responses_output_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiResponsesTotalTokens).sum(),
                "openai_responses_total_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiResponsesInputCachedTokens).sum(),
                "openai_responses_input_cached_tokens",
            )
            .column_as(
                Expr::col(Column::OpenaiResponsesOutputReasoningTokens).sum(),
                "openai_responses_output_reasoning_tokens",
            )
            .filter(Column::CredentialId.eq(credential_id))
            .filter(Column::CreatedAt.gte(start_at))
            .filter(Column::CreatedAt.lte(end_at));

        if let Some(model) = model {
            query = query.filter(Column::Model.eq(model));
        }

        let result = query
            .into_model::<UpstreamUsageAggregate>()
            .one(&self.db)
            .await?;
        Ok(result.unwrap_or_default())
    }

    pub async fn list_downstream_traffic(
        &self,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<entities::downstream_traffic::Model>, u64), DbErr> {
        use entities::downstream_traffic::Column;

        let page = std::cmp::Ord::max(page, 1);
        let page_size = std::cmp::Ord::max(page_size, 1);
        let paginator = entities::DownstreamTraffic::find()
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .paginate(&self.db, page_size);
        let num_pages = paginator.num_pages().await?;
        let items = if num_pages == 0 || page > num_pages {
            Vec::new()
        } else {
            paginator.fetch_page(page - 1).await?
        };
        Ok((items, num_pages))
    }

    pub async fn list_upstream_traffic(
        &self,
        page: u64,
        page_size: u64,
    ) -> Result<(Vec<entities::upstream_traffic::Model>, u64), DbErr> {
        use entities::upstream_traffic::Column;

        let page = std::cmp::Ord::max(page, 1);
        let page_size = std::cmp::Ord::max(page_size, 1);
        let paginator = entities::UpstreamTraffic::find()
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .paginate(&self.db, page_size);
        let num_pages = paginator.num_pages().await?;
        let items = if num_pages == 0 || page > num_pages {
            Vec::new()
        } else {
            paginator.fetch_page(page - 1).await?
        };
        Ok((items, num_pages))
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

    pub async fn get_global_config(
        &self,
    ) -> Result<Option<entities::global_config::Model>, DbErr> {
        entities::GlobalConfig::find()
            .order_by_asc(entities::global_config::Column::Id)
            .one(&self.db)
            .await
    }

    pub async fn list_providers(&self) -> Result<Vec<entities::providers::Model>, DbErr> {
        entities::Providers::find().all(&self.db).await
    }

    pub async fn upsert_provider(&self, input: AdminProviderInput) -> Result<i64, DbErr> {
        use entities::providers::Column;
        let now = OffsetDateTime::now_utc();
        let input_id = input.id;
        let active = entities::providers::ActiveModel {
            id: match input_id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            name: ActiveValue::Set(input.name),
            config_json: ActiveValue::Set(input.config_json),
            enabled: ActiveValue::Set(input.enabled),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };

        let result = entities::Providers::insert(active)
            .on_conflict(
                OnConflict::column(Column::Id)
                    .update_columns([
                        Column::Name,
                        Column::ConfigJson,
                        Column::Enabled,
                        Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(input_id.unwrap_or(result.last_insert_id))
    }

    pub async fn delete_provider(&self, id: i64) -> Result<(), DbErr> {
        entities::Providers::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    pub async fn list_credentials(&self) -> Result<Vec<entities::credentials::Model>, DbErr> {
        entities::Credentials::find().all(&self.db).await
    }

    pub async fn upsert_credential(
        &self,
        input: AdminCredentialInput,
    ) -> Result<(), DbErr> {
        use entities::credentials::Column;
        let now = OffsetDateTime::now_utc();
        let active = entities::credentials::ActiveModel {
            id: match input.id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            provider_id: ActiveValue::Set(input.provider_id),
            name: ActiveValue::Set(input.name),
            secret: ActiveValue::Set(input.secret),
            meta_json: ActiveValue::Set(input.meta_json),
            weight: ActiveValue::Set(input.weight),
            enabled: ActiveValue::Set(input.enabled),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };

        entities::Credentials::insert(active)
            .on_conflict(
                OnConflict::column(Column::Id)
                    .update_columns([
                        Column::ProviderId,
                        Column::Name,
                        Column::Secret,
                        Column::MetaJson,
                        Column::Weight,
                        Column::Enabled,
                        Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn delete_credential(&self, id: i64) -> Result<(), DbErr> {
        entities::Credentials::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    pub async fn list_disallow(
        &self,
    ) -> Result<Vec<entities::credential_disallow::Model>, DbErr> {
        entities::CredentialDisallow::find().all(&self.db).await
    }

    pub async fn upsert_disallow(
        &self,
        input: AdminDisallowInput,
    ) -> Result<(), DbErr> {
        use entities::credential_disallow::Column;
        let now = OffsetDateTime::now_utc();
        let active = entities::credential_disallow::ActiveModel {
            id: ActiveValue::NotSet,
            credential_id: ActiveValue::Set(input.credential_id),
            scope_kind: ActiveValue::Set(input.scope_kind),
            scope_value: ActiveValue::Set(input.scope_value),
            level: ActiveValue::Set(input.level),
            until_at: ActiveValue::Set(input.until_at),
            reason: ActiveValue::Set(input.reason),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };

        entities::CredentialDisallow::insert(active)
            .on_conflict(
                OnConflict::columns([Column::CredentialId, Column::ScopeKind, Column::ScopeValue])
                    .update_columns([Column::Level, Column::UntilAt, Column::Reason, Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn delete_disallow(&self, id: i64) -> Result<(), DbErr> {
        entities::CredentialDisallow::delete_by_id(id)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn list_users(&self) -> Result<Vec<entities::users::Model>, DbErr> {
        entities::Users::find().all(&self.db).await
    }

    pub async fn upsert_user(&self, input: AdminUserInput) -> Result<(), DbErr> {
        use entities::users::Column;
        let now = OffsetDateTime::now_utc();
        let active = entities::users::ActiveModel {
            id: match input.id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            name: ActiveValue::Set(input.name),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
            ..Default::default()
        };

        entities::Users::insert(active)
            .on_conflict(
                OnConflict::column(Column::Id)
                    .update_columns([Column::Name, Column::UpdatedAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn delete_user(&self, id: i64) -> Result<(), DbErr> {
        entities::Users::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    pub async fn list_keys(&self) -> Result<Vec<entities::api_keys::Model>, DbErr> {
        entities::ApiKeys::find().all(&self.db).await
    }

    pub async fn upsert_key(&self, input: AdminKeyInput) -> Result<(), DbErr> {
        use entities::api_keys::Column;
        let now = OffsetDateTime::now_utc();
        let active = entities::api_keys::ActiveModel {
            id: match input.id {
                Some(id) => ActiveValue::Set(id),
                None => ActiveValue::NotSet,
            },
            user_id: ActiveValue::Set(input.user_id),
            key_value: ActiveValue::Set(input.key_value),
            label: ActiveValue::Set(input.label),
            enabled: ActiveValue::Set(input.enabled),
            created_at: ActiveValue::Set(now),
            last_used_at: ActiveValue::Set(None),
            ..Default::default()
        };

        entities::ApiKeys::insert(active)
            .on_conflict(
                OnConflict::column(Column::Id)
                    .update_columns([
                        Column::UserId,
                        Column::KeyValue,
                        Column::Label,
                        Column::Enabled,
                        Column::LastUsedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn delete_key(&self, id: i64) -> Result<(), DbErr> {
        entities::ApiKeys::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    pub async fn set_key_enabled(&self, id: i64, enabled: bool) -> Result<(), DbErr> {
        let active = entities::api_keys::ActiveModel {
            id: ActiveValue::Set(id),
            enabled: ActiveValue::Set(enabled),
            ..Default::default()
        };
        entities::ApiKeys::update(active).exec(&self.db).await?;
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
            trace_id: ActiveValue::Set(event.trace_id),
            request_method: ActiveValue::Set(event.request_method),
            request_path: ActiveValue::Set(event.request_path),
            request_query: ActiveValue::Set(event.request_query),
            request_headers: ActiveValue::Set(event.request_headers),
            request_body: ActiveValue::Set(event.request_body),
            response_status: ActiveValue::Set(event.response_status),
            response_headers: ActiveValue::Set(event.response_headers),
            response_body: ActiveValue::Set(event.response_body),
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
            trace_id: ActiveValue::Set(event.trace_id),
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
