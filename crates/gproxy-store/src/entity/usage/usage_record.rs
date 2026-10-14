//! Physical upstream usage, once per call. Attribution and cost belong here.
//! No log/configuration FKs: usage is retained independently of capture records.

use gproxy_seaorm::FixedDecimal;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_records")]
pub struct Model {
    /// The first three columns form the `by_user` key: an index over
    /// `(user_id, started_at_ms, request_id)`, the order a user's usage is
    /// paged and scanned in. It replaces the single-column index on `user_id`.
    /// Unique only because the entity macro can express a composite index no
    /// other way (the trailing primary key makes it so), which also means a
    /// column belongs to one such key at most: the key and model filters
    /// cannot get one over `started_at_ms` as well, and use their own
    /// single-column indexes.
    #[sea_orm(unique_key = "by_user")]
    pub user_id: Option<String>,
    #[sea_orm(indexed, unique_key = "by_user")]
    pub started_at_ms: i64,
    /// This physical upstream call ID, never a whole WS connection.
    /// A corresponding capture record need not exist.
    #[sea_orm(primary_key, auto_increment = false, unique_key = "by_user")]
    pub request_id: String,
    #[sea_orm(indexed)]
    pub api_key_id: Option<String>,
    #[sea_orm(indexed)]
    pub model: String,
    pub operation: String,
    #[sea_orm(indexed)]
    pub provider_id: Option<String>,
    #[sea_orm(indexed)]
    pub credential_id: Option<String>,
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub cache_creation_5m_tokens: Option<i64>,
    pub cache_creation_30m_tokens: Option<i64>,
    pub cache_creation_1h_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub image_input_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub image_output_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub image_outputs: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub audio_input_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub cached_audio_input_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub audio_output_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub audio_seconds: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub audio_characters: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub video_input_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub video_tokens: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub video_seconds: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub video_outputs: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub search_units: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub web_searches: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub web_fetches: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub file_searches: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub code_interpreter_sessions: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub tool_calls: Option<FixedDecimal>,
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub requests: Option<FixedDecimal>,
    pub state: Option<String>,
    pub completeness: Option<String>,
    pub actual_service_tier: Option<String>,
    /// Dynamic quantities, pricing dimensions and nested protocol-specific detail only.
    pub metrics: Json,
    /// For subscription requests, the settled charge is denominated in USD.
    #[sea_orm(
        column_type = "BigInteger",
        select_as = "char(32)",
        save_as = "decimal(20,0)"
    )]
    pub cost: Option<FixedDecimal>,
    /// Indexed for retention, which deletes the oldest-ended rows first.
    #[sea_orm(indexed)]
    pub ended_at_ms: Option<i64>,
    /// Metered native call duration, including first-token wait.
    pub duration_ms: Option<i64>,
    /// First generated content latency; absent for historical or buffered calls.
    pub ttft_ms: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Dynamic quantities plus fixed quantity columns, without duplicating them in storage.
    /// Values outside the fixed-point range remain exact in the extension document.
    pub fn quantities(&self) -> std::collections::BTreeMap<String, rust_decimal::Decimal> {
        let mut values: std::collections::BTreeMap<String, rust_decimal::Decimal> = self
            .metrics
            .get("metrics")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if let Some(value) = self.image_input_tokens {
            values.insert("image_input_tokens".into(), value.decimal());
        }
        if let Some(value) = self.image_output_tokens {
            values.insert("image_output_tokens".into(), value.decimal());
        }
        if let Some(value) = self.image_outputs {
            values.insert("image_outputs".into(), value.decimal());
        }
        if let Some(value) = self.audio_input_tokens {
            values.insert("audio_input_tokens".into(), value.decimal());
        }
        if let Some(value) = self.cached_audio_input_tokens {
            values.insert("cached_audio_input_tokens".into(), value.decimal());
        }
        if let Some(value) = self.audio_output_tokens {
            values.insert("audio_output_tokens".into(), value.decimal());
        }
        if let Some(value) = self.audio_seconds {
            values.insert("audio_seconds".into(), value.decimal());
        }
        if let Some(value) = self.audio_characters {
            values.insert("audio_characters".into(), value.decimal());
        }
        if let Some(value) = self.video_input_tokens {
            values.insert("video_input_tokens".into(), value.decimal());
        }
        if let Some(value) = self.video_tokens {
            values.insert("video_tokens".into(), value.decimal());
        }
        if let Some(value) = self.video_seconds {
            values.insert("video_seconds".into(), value.decimal());
        }
        if let Some(value) = self.video_outputs {
            values.insert("video_outputs".into(), value.decimal());
        }
        if let Some(value) = self.search_units {
            values.insert("search_units".into(), value.decimal());
        }
        if let Some(value) = self.web_searches {
            values.insert("web_searches".into(), value.decimal());
        }
        if let Some(value) = self.web_fetches {
            values.insert("web_fetches".into(), value.decimal());
        }
        if let Some(value) = self.file_searches {
            values.insert("file_searches".into(), value.decimal());
        }
        if let Some(value) = self.code_interpreter_sessions {
            values.insert("code_interpreter_sessions".into(), value.decimal());
        }
        if let Some(value) = self.tool_calls {
            values.insert("tool_calls".into(), value.decimal());
        }
        if let Some(value) = self.requests {
            values.insert("requests".into(), value.decimal());
        }
        values
    }
}

impl Model {
    /// Reported count, including exact values beyond the database integer range.
    pub fn input_tokens(&self) -> Option<u64> {
        self.input_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| self.metrics.get("tokens")?.get("input_tokens")?.as_u64())
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn output_tokens(&self) -> Option<u64> {
        self.output_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| self.metrics.get("tokens")?.get("output_tokens")?.as_u64())
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn cached_input_tokens(&self) -> Option<u64> {
        self.cached_input_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| {
                self.metrics
                    .get("tokens")?
                    .get("cached_input_tokens")?
                    .as_u64()
            })
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn cache_creation_5m_tokens(&self) -> Option<u64> {
        self.cache_creation_5m_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| {
                self.metrics
                    .get("tokens")?
                    .get("cache_creation_5m_tokens")?
                    .as_u64()
            })
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn cache_creation_30m_tokens(&self) -> Option<u64> {
        self.cache_creation_30m_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| {
                self.metrics
                    .get("tokens")?
                    .get("cache_creation_30m_tokens")?
                    .as_u64()
            })
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn cache_creation_1h_tokens(&self) -> Option<u64> {
        self.cache_creation_1h_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| {
                self.metrics
                    .get("tokens")?
                    .get("cache_creation_1h_tokens")?
                    .as_u64()
            })
    }
    /// Reported count, including exact values beyond the database integer range.
    pub fn reasoning_tokens(&self) -> Option<u64> {
        self.reasoning_tokens
            .and_then(|v| u64::try_from(v).ok())
            .or_else(|| {
                self.metrics
                    .get("tokens")?
                    .get("reasoning_tokens")?
                    .as_u64()
            })
    }
}
