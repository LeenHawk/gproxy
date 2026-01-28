use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "downstream_traffic")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub created_at: OffsetDateTime,
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
    #[sea_orm(belongs_to, from = "provider_id", to = "id")]
    pub provider_ref: HasOne<super::providers::Entity>,
    #[sea_orm(belongs_to, from = "user_id", to = "id")]
    pub user: HasOne<super::users::Entity>,
    #[sea_orm(belongs_to, from = "key_id", to = "id")]
    pub api_key: HasOne<super::api_keys::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
