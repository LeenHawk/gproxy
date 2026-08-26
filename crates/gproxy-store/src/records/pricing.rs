use rust_decimal::Decimal;
use serde_json::Value;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingTierRecord {
    pub service_tier: Option<String>,
    pub min_prompt_tokens: u64,
    pub multiplier: Option<Decimal>,
    pub input: Option<Decimal>,
    pub output: Option<Decimal>,
    pub cache_read: Option<Decimal>,
    pub cache_creation_5m: Option<Decimal>,
    pub cache_creation_30m: Option<Decimal>,
    pub cache_creation_1h: Option<Decimal>,
    pub image_output: Option<Decimal>,
}

pub fn parse_price_tiers(value: Option<&Value>) -> Result<Vec<PricingTierRecord>, StoreError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let items = value
        .as_array()
        .ok_or_else(|| invalid("must be an array"))?;
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let item = item
                .as_object()
                .ok_or_else(|| invalid(format!("row {index} must be an object")))?;
            let service_tier = item
                .get("service_tier")
                .filter(|value| !value.is_null())
                .map(|value| {
                    value
                        .as_str()
                        .filter(|value| !value.trim().is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            invalid(format!("row {index} service_tier must not be blank"))
                        })
                })
                .transpose()?;
            let has_prompt_threshold = item.contains_key("min_prompt_tokens");
            let min_prompt_tokens = item
                .get("min_prompt_tokens")
                .map(|value| {
                    value.as_u64().ok_or_else(|| {
                        invalid(format!(
                            "row {index} min_prompt_tokens must be a nonnegative integer"
                        ))
                    })
                })
                .transpose()?
                .unwrap_or_default();
            if service_tier.is_none() && !has_prompt_threshold {
                return Err(invalid(format!(
                    "row {index} must select service_tier and/or min_prompt_tokens"
                )));
            }
            let decimal = |name| parse_decimal(item.get(name), index, name);
            Ok(PricingTierRecord {
                service_tier,
                min_prompt_tokens,
                multiplier: decimal("multiplier")?,
                input: decimal("input_price")?,
                output: decimal("output_price")?,
                cache_read: decimal("cache_read_price")?,
                cache_creation_5m: decimal("cache_creation_5m_price")?,
                cache_creation_30m: decimal("cache_creation_30m_price")?,
                cache_creation_1h: decimal("cache_creation_1h_price")?,
                image_output: decimal("image_output_price")?,
            })
        })
        .collect()
}

fn parse_decimal(
    value: Option<&Value>,
    index: usize,
    name: &str,
) -> Result<Option<Decimal>, StoreError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    value
        .as_str()
        .and_then(|value| value.parse::<Decimal>().ok())
        .filter(|value| *value >= Decimal::ZERO)
        .map(Some)
        .ok_or_else(|| {
            invalid(format!(
                "row {index} {name} must be a nonnegative decimal string"
            ))
        })
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::InvalidData {
        field: "tiers_json",
        message: message.into(),
    }
}
