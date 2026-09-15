//! Token-price overrides inside one model rule, all prices per million tokens.
//! Select the highest reached prompt threshold without a service tier, then the
//! highest reached threshold for the actual service tier. Equal thresholds use
//! lower (priority, id). Prompt length includes input plus cache-write tokens.
//! A service-tier explicit price wins; otherwise multiply the context-adjusted
//! base by its multiplier (default 1). None inherits; zero is explicitly free.
//! These are selection contracts for the future settlement implementation.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "price_tiers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub price_rule_id: String,
    /// None is a context tier; otherwise standard, priority, flex, batch, etc.
    pub service_tier: Option<String>,
    /// Nonnegative inclusive threshold, counting cache reads and writes once.
    #[sea_orm(default_value = 0)]
    pub min_prompt_tokens: i64,
    #[sea_orm(default_value = 0)]
    pub priority: i32,
    /// Nonnegative multiplier for inherited token prices in a service tier.
    /// Context-only rows leave this unset. Does not multiply tool/media counts.
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub multiplier: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub input_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub output_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cache_read_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cache_creation_5m_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cache_creation_30m_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cache_creation_1h_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub reasoning_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub image_input_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub image_output_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub audio_input_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub cached_audio_input_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub audio_output_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub video_input_per_million: Option<Decimal>,
    #[sea_orm(column_type = "Decimal(Some((28, 12)))")]
    pub video_per_million: Option<Decimal>,
    #[sea_orm(belongs_to, from = "price_rule_id", to = "id", on_delete = "Cascade")]
    pub rule: BelongsTo<super::price_rule::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
