//! GPT-Live reports cumulative duration, plus independently billed delegated
//! Responses/Decisions results. Never sum repeated duration updates.
use std::collections::{BTreeMap, BTreeSet};

use rust_decimal::Decimal;
use serde_json::Value;

use super::{NormalizedUsage, ResponseUsage, UsageAttempt, UsageCompleteness, openai};

#[derive(Default)]
pub(super) struct LiveStream {
    session_id: Option<String>,
    model: Option<String>,
    seconds: Option<Decimal>,
    closed: bool,
    responses: BTreeMap<String, NormalizedUsage>,
    active: BTreeSet<String>,
}

fn attributed(
    mut usage: NormalizedUsage,
    model: Option<&str>,
    operation: crate::Operation,
) -> NormalizedUsage {
    if let Some(model) = model {
        usage.attempts = vec![UsageAttempt {
            operation: Some(operation),
            model: model.to_owned(),
            usage: Box::new(usage.clone()),
            billable: None,
            started_at_ms: None,
        }];
    } else {
        usage.completeness = UsageCompleteness::Partial;
    }
    usage
}

impl LiveStream {
    pub(super) fn event(&mut self, data: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return false;
        };
        let kind = value["type"].as_str().unwrap_or_default();
        if matches!(
            kind,
            "session.started" | "session.updated" | "session.closed"
        ) {
            if let Some(id) = value.pointer("/session/id").and_then(Value::as_str) {
                self.session_id = Some(id.to_owned());
            }
            if let Some(model) = value.pointer("/session/model").and_then(Value::as_str) {
                self.model = Some(model.to_owned());
            }
        }
        if matches!(kind, "session.usage.updated" | "session.closed")
            && let Some(seconds) = value
                .pointer("/usage/seconds")
                .filter(|v| v.is_number())
                .and_then(|v| v.to_string().parse::<Decimal>().ok())
                .filter(|v| *v >= Decimal::ZERO)
        {
            self.seconds = Some(seconds);
            self.closed = kind == "session.closed";
            return true;
        }
        if kind == "response.event" {
            let event = &value["event"];
            let Some(id) = event.pointer("/response/id").and_then(Value::as_str) else {
                return false;
            };
            match event["type"].as_str() {
                Some("response.created") => {
                    self.active.insert(id.to_owned());
                }
                Some(
                    "response.completed"
                    | "response.failed"
                    | "response.incomplete"
                    | "response.done",
                ) => {
                    if let Some(mut usage) = event
                        .pointer("/response/usage")
                        .and_then(openai::from_usage)
                    {
                        super::common::service_tier(
                            &mut usage,
                            event
                                .pointer("/response/service_tier")
                                .and_then(super::common::label),
                        );
                        self.active.remove(id);
                        self.responses.insert(
                            id.to_owned(),
                            attributed(
                                usage,
                                event.pointer("/response/model").and_then(Value::as_str),
                                crate::Operation::GenerateContent,
                            ),
                        );
                        return true;
                    }
                    self.active.insert(id.to_owned());
                }
                _ => {}
            }
        }
        if kind == "decision.event"
            && value.pointer("/event/type").and_then(Value::as_str) == Some("decision.completed")
        {
            // Decision completions have a stable outer event ID, not a Responses ID.
            if let (Some(id), Some(usage)) = (
                value.get("event_id").and_then(Value::as_str),
                value
                    .pointer("/event/decision/usage")
                    .and_then(openai::from_usage),
            ) {
                self.responses.insert(
                    id.to_owned(),
                    attributed(
                        usage,
                        value
                            .pointer("/event/decision/model")
                            .and_then(Value::as_str),
                        crate::Operation::CreateDecision,
                    ),
                );
                return true;
            }
        }
        false
    }

    pub(super) fn snapshot(&self) -> Option<NormalizedUsage> {
        let mut responses: Vec<_> = self
            .responses
            .iter()
            .map(|(id, usage)| ResponseUsage {
                id: id.clone(),
                usage: Box::new(usage.clone()),
            })
            .collect();
        let audio = self.seconds.map(|seconds| {
            let mut usage = NormalizedUsage {
                completeness: if self.closed {
                    UsageCompleteness::Complete
                } else {
                    UsageCompleteness::Partial
                },
                ..Default::default()
            };
            usage.metrics.insert("audio_seconds".into(), seconds);
            usage
                .dimensions
                .insert("cumulative_audio_seconds".into(), "true".into());
            attributed(usage, self.model.as_deref(), crate::Operation::ConnectLive)
        });
        if let (Some(id), Some(usage)) = (&self.session_id, &audio) {
            responses.push(ResponseUsage {
                id: id.clone(),
                usage: Box::new(usage.clone()),
            });
        }
        if audio.is_none() && responses.is_empty() {
            return None;
        }
        let mut total = NormalizedUsage::aggregate(responses.iter().map(|r| r.usage.as_ref()));
        if self.session_id.is_none()
            && let Some(audio) = audio
        {
            // A captured tail with no identity is partial, never a synthetic session ID.
            total = NormalizedUsage::aggregate([&total, &audio]);
        }
        total.attempts = responses
            .iter()
            .flat_map(|r| r.usage.attempts.clone())
            .collect();
        total.responses = responses;
        if !self.closed || !self.active.is_empty() {
            total.completeness = UsageCompleteness::Partial;
        }
        Some(total)
    }
}
