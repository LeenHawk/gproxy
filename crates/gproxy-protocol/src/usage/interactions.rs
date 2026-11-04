//! Gemini Interactions reports cumulative totals, including at step boundaries.
//! Retrieval/resumption exposes existing totals and must never charge them again.
use serde_json::Value;

use super::{NormalizedUsage, UsageCompleteness, common, common::count};

fn usage(value: &Value, complete: bool) -> Option<NormalizedUsage> {
    let input = count(value, "total_input_tokens");
    let output = count(value, "total_output_tokens");
    let thought = count(value, "total_thought_tokens");
    if input.is_none() && output.is_none() && thought.is_none() {
        return None;
    }
    let cached = count(value, "total_cached_tokens").map(|n| n.min(input.unwrap_or(u64::MAX)));
    let mut result = NormalizedUsage {
        completeness: if complete {
            UsageCompleteness::Complete
        } else {
            UsageCompleteness::Partial
        },
        ..Default::default()
    };
    result.tokens.input_tokens = input.map(|n| n.saturating_sub(cached.unwrap_or_default()));
    result.tokens.cached_input_tokens = cached;
    result.tokens.output_tokens = output.or(thought).map(|_| {
        output
            .unwrap_or_default()
            .saturating_add(thought.unwrap_or_default())
    });
    result.tokens.reasoning_tokens = thought;
    common::metric(
        &mut result,
        "tool_use_prompt_tokens",
        count(value, "total_tool_use_tokens").unwrap_or_default(),
    );
    for entry in value["output_tokens_by_modality"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = match entry["modality"].as_str() {
            Some("image") => "image_output_tokens",
            Some("audio") => "audio_output_tokens",
            _ => continue,
        };
        common::metric(
            &mut result,
            name,
            count(entry, "tokens").unwrap_or_default(),
        );
    }
    common::flag_modalities(&mut result);
    Some(result)
}

fn settled(status: Option<&str>) -> bool {
    matches!(
        status,
        Some(
            "completed"
                | "requires_action"
                | "incomplete"
                | "failed"
                | "cancelled"
                | "requires_continuation"
        )
    )
}

pub(super) fn whole(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let mut result = usage(&value["usage"], settled(value["status"].as_str()))?;
    common::response_model(&mut result, value["model"].as_str().map(str::to_owned));
    common::service_tier(
        &mut result,
        value["service_tier"].as_str().map(str::to_owned),
    );
    Some(result)
}

#[derive(Default)]
pub(super) struct Stream {
    latest: Option<NormalizedUsage>,
    model: Option<String>,
    tier: Option<String>,
    terminal: bool,
}

impl Stream {
    pub(super) fn event(&mut self, data: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return false;
        };
        let interaction = &value["interaction"];
        if let Some(model) = interaction["model"].as_str() {
            self.model = Some(model.to_owned());
        }
        if let Some(tier) = interaction["service_tier"].as_str() {
            self.tier = Some(tier.to_owned());
        }
        if value["event_type"] == "interaction.completed"
            || (value["event_type"] == "interaction.status_update"
                && settled(value["status"].as_str()))
        {
            self.terminal = true;
        }
        // step_usage is for one step; total_usage is the whole interaction.
        // A terminal event without usage must retain the latest total.
        let total = interaction
            .get("usage")
            .or_else(|| value.get("total_usage"))
            .or_else(|| value.get("usage"))
            .or_else(|| value.get("delta").and_then(|d| d.get("total_usage")));
        if let Some(reading) = total.and_then(|total| usage(total, self.terminal)) {
            self.latest = Some(reading);
            true
        } else {
            false
        }
    }

    pub(super) fn reading(&self, ended: bool) -> Option<NormalizedUsage> {
        let mut result = self.latest.clone()?;
        result.completeness = if ended && self.terminal {
            UsageCompleteness::Complete
        } else {
            UsageCompleteness::Partial
        };
        common::response_model(&mut result, self.model.clone());
        common::service_tier(&mut result, self.tier.clone());
        Some(result)
    }
}
