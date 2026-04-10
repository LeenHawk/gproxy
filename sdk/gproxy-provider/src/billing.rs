use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::engine::Usage;
use crate::request::PreparedRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BillingMode {
    #[default]
    Default,
    Flex,
    Scale,
    Priority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingContext {
    pub model_id: String,
    #[serde(default)]
    pub mode: BillingMode,
    #[serde(default)]
    pub tool_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingLineItem {
    pub kind: String,
    pub units: Option<i64>,
    pub unit_price: f64,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingResult {
    pub total_cost: f64,
    pub line_items: Vec<BillingLineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPriceTier {
    pub input_tokens_up_to: i64,
    #[serde(default)]
    pub price_input_tokens: Option<f64>,
    #[serde(default)]
    pub price_output_tokens: Option<f64>,
    #[serde(default)]
    pub price_cache_read_input_tokens: Option<f64>,
    #[serde(default)]
    pub price_cache_creation_input_tokens: Option<f64>,
    #[serde(default)]
    pub price_cache_creation_input_tokens_5min: Option<f64>,
    #[serde(default)]
    pub price_cache_creation_input_tokens_1h: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPrice {
    pub model_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub price_each_call: Option<f64>,
    #[serde(default)]
    pub price_tiers: Vec<ModelPriceTier>,
    #[serde(default)]
    pub flex_price_each_call: Option<f64>,
    #[serde(default)]
    pub flex_price_tiers: Vec<ModelPriceTier>,
    #[serde(default)]
    pub scale_price_each_call: Option<f64>,
    #[serde(default)]
    pub scale_price_tiers: Vec<ModelPriceTier>,
    #[serde(default)]
    pub priority_price_each_call: Option<f64>,
    #[serde(default)]
    pub priority_price_tiers: Vec<ModelPriceTier>,
    #[serde(default)]
    pub tool_call_prices: BTreeMap<String, f64>,
}

pub fn parse_model_prices_json(raw: &str) -> Vec<ModelPrice> {
    let mut models: Vec<ModelPrice> =
        serde_json::from_str(raw).expect("invalid built-in model pricing JSON");
    for model in &mut models {
        model
            .price_tiers
            .sort_by_key(|tier| tier.input_tokens_up_to);
        model
            .flex_price_tiers
            .sort_by_key(|tier| tier.input_tokens_up_to);
        model
            .scale_price_tiers
            .sort_by_key(|tier| tier.input_tokens_up_to);
        model
            .priority_price_tiers
            .sort_by_key(|tier| tier.input_tokens_up_to);
    }
    models
}

pub fn build_billing_context(
    channel_id: &str,
    request: &PreparedRequest,
) -> Option<BillingContext> {
    let model_id = request.model.clone()?;
    let body_json = serde_json::from_slice::<serde_json::Value>(&request.body).ok();
    let mode = detect_billing_mode(channel_id, body_json.as_ref());
    let tool_keys = body_json
        .as_ref()
        .map(|body_json| collect_tool_keys(channel_id, body_json))
        .unwrap_or_default();
    Some(BillingContext {
        model_id,
        mode,
        tool_keys,
    })
}

fn split_model_prices<'a>(
    model_prices: &'a [ModelPrice],
    model_id: &str,
) -> (Option<&'a ModelPrice>, Option<&'a ModelPrice>) {
    let exact_model = model_prices.iter().find(|model| model.model_id == model_id);
    let default_model = model_prices
        .iter()
        .find(|model| model.model_id == "default");
    (exact_model, default_model)
}

fn price_each_call_for_mode(model: &ModelPrice, mode: BillingMode) -> Option<f64> {
    match mode {
        BillingMode::Flex => model.flex_price_each_call.or(model.price_each_call),
        BillingMode::Scale => model.scale_price_each_call.or(model.price_each_call),
        BillingMode::Priority => model.priority_price_each_call.or(model.price_each_call),
        BillingMode::Default => model.price_each_call,
    }
}

fn select_price_each_call(
    exact_model: Option<&ModelPrice>,
    default_model: Option<&ModelPrice>,
    mode: BillingMode,
) -> Option<f64> {
    exact_model
        .and_then(|model| price_each_call_for_mode(model, mode))
        .or_else(|| default_model.and_then(|model| price_each_call_for_mode(model, mode)))
}

fn price_tiers_for_mode(model: &ModelPrice, mode: BillingMode) -> Option<&[ModelPriceTier]> {
    let tiers = match mode {
        BillingMode::Flex if !model.flex_price_tiers.is_empty() => {
            model.flex_price_tiers.as_slice()
        }
        BillingMode::Scale if !model.scale_price_tiers.is_empty() => {
            model.scale_price_tiers.as_slice()
        }
        BillingMode::Priority if !model.priority_price_tiers.is_empty() => {
            model.priority_price_tiers.as_slice()
        }
        _ if !model.price_tiers.is_empty() => model.price_tiers.as_slice(),
        _ => return None,
    };
    Some(tiers)
}

fn select_price_tiers<'a>(
    exact_model: Option<&'a ModelPrice>,
    default_model: Option<&'a ModelPrice>,
    mode: BillingMode,
) -> Option<&'a [ModelPriceTier]> {
    exact_model
        .and_then(|model| price_tiers_for_mode(model, mode))
        .or_else(|| default_model.and_then(|model| price_tiers_for_mode(model, mode)))
}

pub fn estimate_billing(
    model_prices: &[ModelPrice],
    context: &BillingContext,
    usage: &Usage,
) -> Option<BillingResult> {
    let (exact_model, default_model) = split_model_prices(model_prices, &context.model_id);
    if exact_model.is_none() && default_model.is_none() {
        return None;
    }
    let mut total_cost = 0.0;
    let mut line_items = Vec::new();

    let price_each_call = select_price_each_call(exact_model, default_model, context.mode);
    let price_tiers = select_price_tiers(exact_model, default_model, context.mode).unwrap_or(&[]);

    if let Some(price) = price_each_call {
        total_cost += price;
        line_items.push(BillingLineItem {
            kind: "request".to_string(),
            units: Some(1),
            unit_price: price,
            amount: price,
        });
    }

    if let Some(tier) = select_tier(price_tiers, effective_input_tokens(usage)) {
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "input_tokens",
            usage.input_tokens,
            tier.price_input_tokens,
        );
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "output_tokens",
            usage.output_tokens,
            tier.price_output_tokens,
        );
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "cache_read_input_tokens",
            usage.cache_read_input_tokens,
            tier.price_cache_read_input_tokens,
        );
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "cache_creation_input_tokens",
            usage.cache_creation_input_tokens,
            tier.price_cache_creation_input_tokens,
        );
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "cache_creation_input_tokens_5min",
            usage.cache_creation_input_tokens_5min,
            tier.price_cache_creation_input_tokens_5min,
        );
        push_usage_cost(
            &mut line_items,
            &mut total_cost,
            "cache_creation_input_tokens_1h",
            usage.cache_creation_input_tokens_1h,
            tier.price_cache_creation_input_tokens_1h,
        );
    }

    for tool_key in &context.tool_keys {
        if let Some(price) = exact_model
            .and_then(|model| model.tool_call_prices.get(tool_key))
            .copied()
            .or_else(|| {
                default_model
                    .and_then(|model| model.tool_call_prices.get(tool_key))
                    .copied()
            })
        {
            total_cost += price;
            line_items.push(BillingLineItem {
                kind: format!("tool:{tool_key}"),
                units: Some(1),
                unit_price: price,
                amount: price,
            });
        }
    }

    Some(BillingResult {
        total_cost,
        line_items,
    })
}

pub fn estimate_cost(
    model_prices: &[ModelPrice],
    context: &BillingContext,
    usage: &Usage,
) -> Option<f64> {
    estimate_billing(model_prices, context, usage).map(|result| result.total_cost)
}

fn select_tier(tiers: &[ModelPriceTier], input_tokens: i64) -> Option<&ModelPriceTier> {
    tiers
        .iter()
        .find(|tier| input_tokens <= tier.input_tokens_up_to)
        .or_else(|| tiers.last())
}

fn effective_input_tokens(usage: &Usage) -> i64 {
    usage.input_tokens.unwrap_or(0)
        + usage.cache_read_input_tokens.unwrap_or(0)
        + usage.cache_creation_input_tokens.unwrap_or(0)
        + usage.cache_creation_input_tokens_5min.unwrap_or(0)
        + usage.cache_creation_input_tokens_1h.unwrap_or(0)
}

fn push_usage_cost(
    line_items: &mut Vec<BillingLineItem>,
    total_cost: &mut f64,
    kind: &str,
    units: Option<i64>,
    unit_price: Option<f64>,
) {
    let (Some(units), Some(unit_price)) = (units, unit_price) else {
        return;
    };
    let amount = units as f64 * unit_price / 1_000_000.0;
    *total_cost += amount;
    line_items.push(BillingLineItem {
        kind: kind.to_string(),
        units: Some(units),
        unit_price,
        amount,
    });
}

fn detect_billing_mode(channel_id: &str, body_json: Option<&serde_json::Value>) -> BillingMode {
    let Some(body_json) = body_json else {
        return BillingMode::Default;
    };
    match channel_id {
        "openai" => {
            match body_json
                .get("service_tier")
                .and_then(serde_json::Value::as_str)
            {
                Some("flex") => BillingMode::Flex,
                Some("scale") => BillingMode::Scale,
                Some("priority") => BillingMode::Priority,
                _ => BillingMode::Default,
            }
        }
        "anthropic" | "claudecode" => {
            if body_json.get("speed").and_then(serde_json::Value::as_str) == Some("fast") {
                BillingMode::Priority
            } else {
                BillingMode::Default
            }
        }
        _ => BillingMode::Default,
    }
}

fn collect_tool_keys(channel_id: &str, body_json: &serde_json::Value) -> Vec<String> {
    let mut tool_keys = Vec::new();
    let Some(tools) = body_json.get("tools").and_then(serde_json::Value::as_array) else {
        return tool_keys;
    };

    for tool in tools {
        match channel_id {
            "aistudio" | "vertex" | "vertexexpress" | "geminicli" | "antigravity" => {
                if tool.get("google_search").is_some() {
                    tool_keys.push("google_search".to_string());
                }
                if tool.get("google_search_retrieval").is_some() {
                    tool_keys.push("google_search_retrieval".to_string());
                }
                if tool.get("googleMaps").is_some() || tool.get("google_maps").is_some() {
                    tool_keys.push("google_maps".to_string());
                }
                if tool.get("code_execution").is_some() {
                    tool_keys.push("code_execution".to_string());
                }
                if tool.get("url_context").is_some() {
                    tool_keys.push("url_context".to_string());
                }
            }
            "anthropic" | "claudecode" => {
                if let Some(tool_type) = tool.get("type").and_then(serde_json::Value::as_str) {
                    if tool_type.starts_with("web_search") {
                        tool_keys.push("web_search".to_string());
                    } else if tool_type.starts_with("web_fetch") {
                        tool_keys.push("web_fetch".to_string());
                    } else if tool_type.starts_with("code_execution") {
                        tool_keys.push("code_execution".to_string());
                    } else if tool_type.starts_with("text_editor") {
                        tool_keys.push("text_editor".to_string());
                    } else if tool_type == "bash" {
                        tool_keys.push("bash".to_string());
                    } else {
                        tool_keys.push(tool_type.to_string());
                    }
                }
            }
            _ => {
                if let Some(tool_type) = tool.get("type").and_then(serde_json::Value::as_str) {
                    if tool_type.starts_with("web_search_preview") {
                        tool_keys.push("web_search_preview".to_string());
                    } else if tool_type.starts_with("web_search") {
                        tool_keys.push("web_search".to_string());
                    } else if tool_type.starts_with("web_fetch") {
                        tool_keys.push("web_fetch".to_string());
                    } else if tool_type.starts_with("code_execution") {
                        tool_keys.push("code_execution".to_string());
                    } else if tool_type == "file_search" {
                        tool_keys.push("file_search".to_string());
                    } else if tool_type == "code_interpreter" {
                        tool_keys.push("code_interpreter".to_string());
                    } else {
                        tool_keys.push(tool_type.to_string());
                    }
                }
            }
        }
    }

    tool_keys.sort();
    tool_keys.dedup();
    tool_keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_cost_from_tiered_prices() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "test-model",
                "price_each_call": 0.5,
                "priority_price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 10.0
                  }
                ],
                "tool_call_prices": {
                  "web_search": 0.01
                },
                "price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 1.0,
                    "price_output_tokens": 2.0,
                    "price_cache_read_input_tokens": 0.1
                  }
                ]
              }
            ]
            "#,
        );
        let usage = Usage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_read_input_tokens: Some(200),
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        };
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: vec!["web_search".to_string()],
        };

        let cost = estimate_cost(&prices, &context, &usage).unwrap();
        assert!((cost - 0.512_02).abs() < 1e-9);
        let priority_context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Priority,
            tool_keys: Vec::new(),
        };
        let priority_cost = estimate_cost(&prices, &priority_context, &usage).unwrap();
        assert!((priority_cost - 0.51).abs() < 1e-9);
    }

    #[test]
    fn exact_model_price_beats_default_fallback() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "price_each_call": 0.25
              },
              {
                "model_id": "test-model",
                "price_each_call": 1.5
              }
            ]
            "#,
        );
        let usage = Usage::default();
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(1.5));
    }

    #[test]
    fn exact_model_without_pricing_falls_back_to_default_price_each_call() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "price_each_call": 0.25
              },
              {
                "model_id": "test-model"
              }
            ]
            "#,
        );
        let usage = Usage::default();
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.25));
    }

    #[test]
    fn exact_model_without_tiers_falls_back_to_default_tiers() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 1.0,
                    "price_output_tokens": 2.0
                  }
                ]
              },
              {
                "model_id": "test-model",
                "price_each_call": 0.5
              }
            ]
            "#,
        );
        let usage = Usage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        };
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.502));
    }

    #[test]
    fn exact_model_without_priority_tiers_falls_back_to_default_priority_tiers() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "priority_price_each_call": 0.9,
                "priority_price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 10.0
                  }
                ]
              },
              {
                "model_id": "test-model",
                "priority_price_each_call": 1.0
              }
            ]
            "#,
        );
        let usage = Usage {
            input_tokens: Some(1000),
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        };
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Priority,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(1.01));
    }

    #[test]
    fn exact_model_missing_tool_price_uses_default_tool_price() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "tool_call_prices": {
                  "web_search": 0.01
                }
              },
              {
                "model_id": "test-model",
                "price_each_call": 0.5
              }
            ]
            "#,
        );
        let usage = Usage::default();
        let context = BillingContext {
            model_id: "test-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: vec!["web_search".to_string()],
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.51));
    }

    #[test]
    fn missing_model_uses_default_price_each_call() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "price_each_call": 0.25
              }
            ]
            "#,
        );
        let usage = Usage::default();
        let context = BillingContext {
            model_id: "missing-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.25));
    }

    #[test]
    fn missing_model_uses_default_price_tiers() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 1.0,
                    "price_output_tokens": 2.0
                  }
                ]
              }
            ]
            "#,
        );
        let usage = Usage {
            input_tokens: Some(1000),
            output_tokens: Some(500),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        };
        let context = BillingContext {
            model_id: "missing-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.002));
    }

    #[test]
    fn missing_model_uses_default_priority_prices() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "default",
                "priority_price_each_call": 0.9,
                "priority_price_tiers": [
                  {
                    "input_tokens_up_to": 1000,
                    "price_input_tokens": 10.0
                  }
                ]
              }
            ]
            "#,
        );
        let usage = Usage {
            input_tokens: Some(1000),
            output_tokens: None,
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
        };
        let context = BillingContext {
            model_id: "missing-model".to_string(),
            mode: BillingMode::Priority,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), Some(0.91));
    }

    #[test]
    fn missing_model_without_default_still_returns_none() {
        let prices = parse_model_prices_json(
            r#"
            [
              {
                "model_id": "some-other-model",
                "price_each_call": 1.0
              }
            ]
            "#,
        );
        let usage = Usage::default();
        let context = BillingContext {
            model_id: "missing-model".to_string(),
            mode: BillingMode::Default,
            tool_keys: Vec::new(),
        };

        assert_eq!(estimate_cost(&prices, &context, &usage), None);
    }
}
