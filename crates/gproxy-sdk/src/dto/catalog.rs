//! The static catalogues: what this build knows without asking a database.
//!
//! Three kinds of fixed data a management UI renders forms from. The channel
//! descriptors come from the compiled-in channels; the default model catalog
//! is a bundled asset; the TLS and rule presets are tables in this crate. None
//! of them is configuration — nothing here is stored until a caller applies it.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ---------------------------------------------------------------------------
// The bundled model catalog.
// ---------------------------------------------------------------------------

/// The bundled catalog, exactly as the asset spells it. Its numbers are
/// checked against its own rows at load: a truncated asset is a build problem
/// and should be found once, not per lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelCatalogDto {
    pub schema_version: u32,
    pub source: DefaultModelCatalogSourceDto,
    pub models: Vec<DefaultModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelCatalogSourceDto {
    pub catalog: String,
    pub fetched_at: String,
    pub total_models: usize,
    pub priced_models: usize,
    /// Everything else the generator recorded about the snapshot. Carried so a
    /// console can show it without this crate tracking the generator's fields.
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

/// One catalog entry. `model_id` is the OpenRouter-style `vendor/name`; the
/// unmodelled keys — modalities, supported parameters, reasoning levels — stay
/// in `metadata`, which is exactly the shape the `models.metadata` column
/// holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelDto {
    pub model_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub context_window: Option<i64>,
    #[serde(default)]
    pub max_output_tokens: Option<i64>,
    #[serde(default)]
    pub pricing: Option<DefaultModelPricingDto>,
    #[serde(flatten)]
    pub metadata: Map<String, Value>,
}

/// The price book for one model: a glob, its rates, and the context or
/// service-tier overrides above them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelPricingDto {
    /// A `*fragment*` glob. The longer the fragment, the more specific the
    /// rule, which is what `priority` encodes.
    pub model_pattern: String,
    pub priority: i64,
    pub rates: Vec<DefaultModelPriceRateDto>,
    #[serde(default)]
    pub tiers: Option<Vec<DefaultModelTierDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelPriceRateDto {
    pub metric: String,
    /// `1000000` for the token metrics, `1` for counted ones.
    pub unit_size: u64,
    /// A decimal string: this is money and must not pass through a float.
    pub price: String,
    pub priority: i64,
}

/// A long-context or service-tier override. Every price is optional and every
/// absent one inherits the rule's rates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct DefaultModelTierDto {
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub min_prompt_tokens: Option<i64>,
    #[serde(default)]
    pub multiplier: Option<String>,
    #[serde(default)]
    pub input_price: Option<String>,
    #[serde(default)]
    pub output_price: Option<String>,
    #[serde(default)]
    pub cache_read_price: Option<String>,
    #[serde(default)]
    pub cache_creation_5m_price: Option<String>,
    #[serde(default)]
    pub cache_creation_30m_price: Option<String>,
    #[serde(default)]
    pub cache_creation_1h_price: Option<String>,
    #[serde(default)]
    pub image_output_price: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDefaultPricesRequest {
    /// None writes global rules, which price the model at every provider. A
    /// provider id writes rules that only that provider's traffic matches, and
    /// those are written under the literal model id rather than the catalog's
    /// glob.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Catalog `model_id`s, or names a catalog glob covers.
    pub model_ids: Vec<String>,
    /// Whether a rule that already covers the same pattern in the same scope
    /// is replaced. False leaves an operator's edited price alone, which is
    /// what makes re-applying the catalog safe.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDefaultPricesReportDto {
    pub created: u64,
    /// Rules `overwrite` replaced, rates and tiers included.
    pub updated: u64,
    /// Rules already present that `overwrite: false` left alone.
    pub skipped: u64,
    /// Names no catalog entry covers.
    pub unmatched: u64,
}

// ---------------------------------------------------------------------------
// TLS presets.
// ---------------------------------------------------------------------------

/// One client identity, ready to be stored as a connection profile's
/// `emulation`. The object is a `gproxy_client::EmulationConfig`, so a caller
/// copies it across unchanged rather than translating anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsPresetDto {
    pub id: String,
    pub label: String,
    /// Only meaningful with the `wreq` backend; the others reject an emulation.
    pub emulation: Value,
}

// ---------------------------------------------------------------------------
// Rewrite rule presets.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulePresetCategory {
    /// Rewrites that hide one client application's identity from the upstream.
    Application,
}

/// A named set of rewrite rules. `rules` is what `apply_rule_preset` writes;
/// it is the ordinary write shape, so a caller may edit the list first and
/// send it through `rewrite().replace_rules` itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePresetDto {
    pub id: String,
    pub name: String,
    /// A stable machine tag, `gproxy:preset:{id}:v1`, so a console can tell
    /// which preset a rule set came from.
    pub description: String,
    pub category: RulePresetCategory,
    pub rules: Vec<super::RewriteRuleWrite>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRulePreset {
    pub rule_set_id: String,
    pub preset_id: String,
}
