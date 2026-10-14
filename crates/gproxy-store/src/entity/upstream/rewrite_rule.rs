//! One ordered rule: text, JSON, native content, or HTTP header operations.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "rewrite_rules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub rule_set_id: String,
    /// request, response, or both; relative to the upstream connection.
    #[sea_orm(default_value = "request")]
    pub phase: String,
    /// replace, set/delete/merge, system_text/cache_breakpoint, or header_set/header_merge.
    #[sea_orm(default_value = "replace")]
    pub action: String,
    /// Body/payload text, named header values, or named request-query values.
    /// Defaults to body so that a migration adding this column leaves existing
    /// rules meaning what they meant.
    #[sea_orm(default_value = "body")]
    pub target: RewriteTarget,
    /// Required for header/query targets, absent for body. Header names match
    /// case-insensitively; query names match exactly after decoding.
    pub target_name: Option<String>,
    /// Optional JSON array of dot paths, e.g. ["tools.*.name"].
    /// Body only: None selects all payload text; paths select JSON string values.
    /// Must be absent for header/query targets.
    pub paths: Option<Json>,
    /// Rust regex syntax, including inline flags such as (?i).
    #[sea_orm(column_type = "Text")]
    pub pattern: String,
    /// Replacement text, a typed JSON value, or JSON config for native content actions.
    #[sea_orm(column_type = "Text")]
    pub replacement: String,
    /// Optional JSON array of {"operation": ..., "dialect": ...} pairs.
    pub filter_operation_keys: Option<Json>,
    /// Glob against the selected upstream model or the requested model alias.
    #[sea_orm(column_type = "Text")]
    pub filter_model_pattern: Option<String>,
    /// Case-insensitive regex against inbound request header lines.
    #[sea_orm(column_type = "Text")]
    pub filter_header_pattern: Option<String>,
    /// Regex against the SSE event name (falling back to JSON type), or WS JSON type.
    #[sea_orm(column_type = "Text")]
    pub filter_event_pattern: Option<String>,
    /// JMESPath expression string or legacy field comparison; body targets only.
    pub filter_body: Option<Json>,
    /// Typed comparison against an original inbound request header.
    pub filter_header: Option<Json>,
    /// Ascending order within the rule set; id breaks ties.
    #[sea_orm(default_value = 0)]
    pub sort_order: i64,
    #[sea_orm(default_value = true)]
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    #[sea_orm(belongs_to, from = "rule_set_id", to = "id", on_delete = "Cascade")]
    pub rule_set: BelongsTo<super::rewrite_rule_set::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    EnumIter,
    DeriveActiveEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum RewriteTarget {
    #[default]
    #[sea_orm(string_value = "body")]
    Body,
    #[sea_orm(string_value = "header")]
    Header,
    #[sea_orm(string_value = "query")]
    Query,
}
