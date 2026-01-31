use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_traffic")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub created_at: OffsetDateTime,
    pub provider: String,
    pub provider_id: Option<i64>,
    pub operation: String,
    pub model: Option<String>,
    pub credential_id: Option<i64>,
    pub trace_id: Option<String>,
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
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "SetNull")]
    pub provider_ref: HasOne<super::providers::Entity>,
    #[sea_orm(belongs_to, from = "credential_id", to = "id", on_delete = "SetNull")]
    pub credential: HasOne<super::credentials::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
