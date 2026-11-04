//! Bounded native Interactions event collection and lifecycle synthesis.
use super::*;
use crate::wire::{DeclaredFields, gemini::interactions as i};
use std::collections::{BTreeMap, BTreeSet};

pub struct InteractionStreamCollector {
    interaction: Option<Value>,
    steps: BTreeMap<u64, Value>,
    open: BTreeSet<u64>,
    arguments: BTreeMap<u64, String>,
    usage: Option<Value>,
    terminal: bool,
    bytes: usize,
    max_bytes: usize,
}

impl InteractionStreamCollector {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            interaction: None,
            steps: BTreeMap::new(),
            open: BTreeSet::new(),
            arguments: BTreeMap::new(),
            usage: None,
            terminal: false,
            bytes: 0,
            max_bytes,
        }
    }
    pub fn push(&mut self, event: i::InteractionStreamEvent) -> Result<(), TransformError> {
        let event = serde_json::to_value(event.into_declared())?;
        self.bytes = self.bytes.saturating_add(event.to_string().len());
        if self.bytes > self.max_bytes {
            return Err(TransformError::new(
                super::super::TransformErrorKind::Limit,
                "interaction.stream",
                "event budget exceeded",
            ));
        }
        if self.terminal {
            return Err(TransformError::invalid_result(
                "interaction.stream",
                "event after terminal interaction",
            ));
        }
        if let Some(usage) = event
            .get("total_usage")
            .or_else(|| event.get("delta").and_then(|d| d.get("total_usage")))
        {
            self.usage = Some(usage.clone());
        }
        match string(&event["event_type"]) {
            "error" => {
                return Err(TransformError::upstream(
                    "interaction.error",
                    serde_json::from_value::<i::InteractionError>(event["error"].clone())?,
                ));
            }
            "interaction.created" => {
                if self.interaction.is_some() {
                    return Err(TransformError::invalid_result(
                        "interaction.created",
                        "duplicate interaction",
                    ));
                }
                self.interaction = Some(event["interaction"].clone());
            }
            "interaction.completed" => {
                let prior = self.interaction.as_ref().ok_or_else(|| {
                    TransformError::invalid_result("interaction.completed", "missing created event")
                })?;
                if prior["id"] != event["interaction"]["id"] {
                    return Err(TransformError::invalid_result(
                        "interaction.id",
                        "terminal id changed",
                    ));
                }
                self.interaction = Some(event["interaction"].clone());
                self.terminal = true;
            }
            "interaction.status_update" => {
                let prior = self.interaction.as_mut().ok_or_else(|| {
                    TransformError::invalid_result(
                        "interaction.status_update",
                        "missing created event",
                    )
                })?;
                if prior["id"] != event["interaction_id"] {
                    return Err(TransformError::invalid_result(
                        "interaction.id",
                        "status id changed",
                    ));
                }
                prior["status"] = event["status"].clone();
                if let Some(usage) = event.get("usage") {
                    self.usage = Some(usage.clone());
                }
            }
            "step.start" => {
                let index = event["index"].as_u64().ok_or_else(|| {
                    TransformError::invalid_result("step.index", "index required")
                })?;
                if self.steps.contains_key(&index) || self.interaction.is_none() {
                    return Err(TransformError::invalid_result(
                        "step.start",
                        "duplicate step or missing interaction",
                    ));
                }
                if !event["step"].is_object() {
                    return Err(TransformError::invalid_result(
                        "step.start",
                        "a native step is required",
                    ));
                }
                self.steps.insert(index, event["step"].clone());
                self.open.insert(index);
            }
            "step.delta" => {
                let index = event["index"].as_u64().ok_or_else(|| {
                    TransformError::invalid_result("step.index", "index required")
                })?;
                if !self.open.contains(&index) {
                    return Err(TransformError::invalid_result(
                        "step.delta",
                        "step is not open",
                    ));
                }
                if !event["delta"].is_object() {
                    return Err(TransformError::invalid_result(
                        "step.delta",
                        "a native delta is required",
                    ));
                }
                let step = self.steps.get_mut(&index).expect("open step");
                let delta = &event["delta"];
                match string(&delta["type"]) {
                    "text" => append_content(step, "content", string(&delta["text"])),
                    "thought_summary" => {
                        append_content(step, "summary", string(&delta["content"]["text"]))
                    }
                    "thought_signature" => {
                        step["signature"] = delta["signature"].clone();
                    }
                    "arguments_delta" => {
                        self.arguments
                            .entry(index)
                            .or_default()
                            .push_str(string(&delta["arguments"]));
                    }
                    "text_annotation_delta" => {}
                    kind => {
                        for (field, value) in delta.as_object().into_iter().flatten() {
                            if field != "type" && field != "total_usage" {
                                step[field] = value.clone();
                            }
                        }
                        if matches!(kind, "image" | "audio" | "video" | "document") {
                            let mut part = delta.clone();
                            part.as_object_mut().expect("delta").remove("total_usage");
                            let contents = step
                                .as_object_mut()
                                .expect("step")
                                .entry("content")
                                .or_insert_with(|| json!([]));
                            contents
                                .as_array_mut()
                                .ok_or_else(|| {
                                    TransformError::invalid_result(
                                        "step.content",
                                        "content is not an array",
                                    )
                                })?
                                .push(part);
                        }
                    }
                }
            }
            "step.stop" => {
                let index = event["index"].as_u64().ok_or_else(|| {
                    TransformError::invalid_result("step.index", "index required")
                })?;
                if !self.open.remove(&index) {
                    return Err(TransformError::invalid_result(
                        "step.stop",
                        "step is not open",
                    ));
                }
                if let Some(args) = self.arguments.remove(&index) {
                    self.steps.get_mut(&index).expect("step")["arguments"] =
                        serde_json::from_str(&args).map_err(|e| {
                            TransformError::invalid_result("step.arguments", e.to_string())
                        })?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Converted<i::Interaction>, TransformError> {
        if !self.terminal || !self.open.is_empty() {
            return Err(TransformError::invalid_result(
                "interaction.stream",
                "missing terminal interaction or unfinished step",
            ));
        }
        let mut interaction = self.interaction.ok_or_else(|| {
            TransformError::invalid_result("interaction.stream", "missing interaction")
        })?;
        if !self.steps.is_empty() {
            if self.steps.keys().copied().ne(0..self.steps.len() as u64) {
                return Err(TransformError::invalid_result(
                    "step.index",
                    "steps are not contiguous",
                ));
            }
            let steps: Vec<_> = self.steps.into_values().collect();
            if interaction["steps"].as_array().is_none_or(Vec::is_empty) {
                interaction["steps"] = json!(steps);
            }
        }
        if interaction["usage"].is_null()
            && let Some(usage) = self.usage
        {
            interaction["usage"] = usage;
        }
        Ok(Converted {
            value: serde_json::from_value(interaction)?,
            report: Report::default(),
        })
    }
}

fn append_content(step: &mut Value, field: &str, text: &str) {
    let parts = step
        .as_object_mut()
        .expect("step object")
        .entry(field)
        .or_insert_with(|| json!([]));
    if parts.as_array().is_none() {
        *parts = json!([]);
    }
    let parts = parts.as_array_mut().expect("array");
    if let Some(last) = parts.last_mut().filter(|part| part["type"] == "text") {
        let mut value = string(&last["text"]).to_owned();
        value.push_str(text);
        last["text"] = json!(value);
    } else {
        parts.push(json!({"type":"text","text":text}));
    }
}

pub fn synthesize_interaction_stream(
    interaction: i::Interaction,
    max_bytes: usize,
) -> Result<Converted<Vec<i::InteractionStreamEvent>>, TransformError> {
    let interaction = serde_json::to_value(interaction.into_declared())?;
    let mut initial = interaction.clone();
    initial["steps"] = json!([]);
    initial["status"] = json!("in_progress");
    initial.as_object_mut().expect("object").remove("usage");
    let mut events = vec![json!({"event_type":"interaction.created","interaction":initial})];
    for (index, step) in array(&interaction["steps"]).enumerate() {
        events.push(json!({"event_type":"step.start","index":index,"step":step}));
        events.push(json!({"event_type":"step.stop","index":index}));
    }
    events.push(json!({"event_type":"interaction.completed","interaction":interaction}));
    let mut bytes = 0usize;
    let events = events
        .into_iter()
        .map(|event| {
            bytes = bytes.saturating_add(event.to_string().len());
            if bytes > max_bytes {
                return Err(TransformError::new(
                    super::super::TransformErrorKind::Limit,
                    "interaction.stream",
                    "synthesis budget exceeded",
                ));
            }
            Ok(serde_json::from_value(event)?)
        })
        .collect::<Result<Vec<_>, TransformError>>()?;
    Ok(Converted {
        value: events,
        report: Report::default(),
    })
}
