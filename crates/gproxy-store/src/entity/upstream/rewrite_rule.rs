//! One ordered regex replacement, separate from protocol conversion overrides.

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
    /// Optional JSON array of dot paths, e.g. ["tools.*.name"].
    /// None selects the entire payload text; paths select JSON string values.
    pub paths: Option<Json>,
    /// Rust regex syntax, including inline flags such as (?i).
    #[sea_orm(column_type = "Text")]
    pub pattern: String,
    /// Regex replacement syntax, including $1 and ${name} capture references.
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
