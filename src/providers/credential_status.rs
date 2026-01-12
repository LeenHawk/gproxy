use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use serde::de::{self, MapAccess, Visitor};
use time::OffsetDateTime;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, sleep};

use crate::context::AppContext;
use crate::usage::{next_anchor_ts, slot_secs_for_view, slot_start};

const BASE_BACKOFF_SECS: i64 = 5;
const MAX_BACKOFF_SECS: i64 = 24 * 60 * 60;

pub const DEFAULT_MODEL_KEY: &str = "*";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAI,
    Claude,
    AIStudio,
    DeepSeek,
    Nvidia,
    VertexExpress,
    ClaudeCode,
    Codex,
    Vertex,
    GeminiCli,
    Antigravity,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::OpenAI => "openai",
            ProviderKind::Claude => "claude",
            ProviderKind::AIStudio => "aistudio",
            ProviderKind::DeepSeek => "deepseek",
            ProviderKind::Nvidia => "nvidia",
            ProviderKind::VertexExpress => "vertexexpress",
            ProviderKind::ClaudeCode => "claudecode",
            ProviderKind::Codex => "codex",
            ProviderKind::Vertex => "vertex",
            ProviderKind::GeminiCli => "geminicli",
            ProviderKind::Antigravity => "antigravity",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CredentialStatus {
    #[default]
    Active,
    Cooldown { until: i64 },
    Transient { until: i64 },
    Disabled,
}

impl<'de> Deserialize<'de> for CredentialStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StatusVisitor;

        impl<'de> Visitor<'de> for StatusVisitor {
            type Value = CredentialStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("credential status map")
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CredentialStatus::Active)
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(CredentialStatus::Active)
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut state: Option<String> = None;
                let mut until: Option<i64> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "state" => state = Some(map.next_value()?),
                        "until" => until = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                if let Some(state) = state {
                    Ok(CredentialStatus::from_parts(&state, until))
                } else {
                    Ok(CredentialStatus::Active)
                }
            }
        }

        deserializer.deserialize_any(StatusVisitor)
    }
}


impl CredentialStatus {
    pub fn is_ready(&self, now: i64) -> bool {
        match self {
            CredentialStatus::Active => true,
            CredentialStatus::Disabled => false,
            CredentialStatus::Cooldown { until }
            | CredentialStatus::Transient { until } => *until <= now,
        }
    }

    pub fn until(&self) -> Option<i64> {
        match self {
            CredentialStatus::Cooldown { until } | CredentialStatus::Transient { until } => {
                Some(*until)
            }
            _ => None,
        }
    }

    pub fn state_name(&self) -> &'static str {
        match self {
            CredentialStatus::Active => "active",
            CredentialStatus::Cooldown { .. } => "cooldown",
            CredentialStatus::Transient { .. } => "transient",
            CredentialStatus::Disabled => "disabled",
        }
    }

    pub fn from_parts(state: &str, until: Option<i64>) -> Self {
        match state {
            "cooldown" => until
                .map(|value| CredentialStatus::Cooldown { until: value })
                .unwrap_or(CredentialStatus::Active),
            "cooldown_sonnet" => until
                .map(|value| CredentialStatus::Cooldown { until: value })
                .unwrap_or(CredentialStatus::Active),
            "transient" => until
                .map(|value| CredentialStatus::Transient { until: value })
                .unwrap_or(CredentialStatus::Active),
            "disabled" => CredentialStatus::Disabled,
            _ => CredentialStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelStatusTuple(pub String, pub String, pub i64);

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(transparent)]
pub struct CredentialStatusList(pub Vec<ModelStatusTuple>);

impl CredentialStatusList {
    pub fn is_ready_for(&self, model: &str, now: i64) -> bool {
        self.effective_status(model).is_ready(now)
    }

    pub fn effective_status(&self, model: &str) -> CredentialStatus {
        let global = self
            .status_for_model(DEFAULT_MODEL_KEY)
            .unwrap_or(CredentialStatus::Active);
        let specific = self
            .status_for_model(model)
            .unwrap_or(CredentialStatus::Active);
        merge_status(&global, &specific)
    }

    pub fn status_for_model(&self, model: &str) -> Option<CredentialStatus> {
        self.0
            .iter()
            .find(|entry| entry.0 == model)
            .map(|entry| {
                let until = if entry.2 <= 0 { None } else { Some(entry.2) };
                CredentialStatus::from_parts(entry.1.as_str(), until)
            })
    }

    pub fn update_status(&mut self, model: &str, status: CredentialStatus) {
        if status == CredentialStatus::Active {
            self.remove_model(model);
            return;
        }
        let state = status.state_name().to_string();
        let until = status.until().unwrap_or(0);
        if let Some(entry) = self.0.iter_mut().find(|entry| entry.0 == model) {
            entry.1 = state;
            entry.2 = until;
        } else {
            self.0.push(ModelStatusTuple(model.to_string(), state, until));
        }
    }

    pub fn remove_model(&mut self, model: &str) {
        self.0.retain(|entry| entry.0 != model);
    }
}

fn merge_status(a: &CredentialStatus, b: &CredentialStatus) -> CredentialStatus {
    match (a, b) {
        (CredentialStatus::Disabled, _) | (_, CredentialStatus::Disabled) => {
            CredentialStatus::Disabled
        }
        (CredentialStatus::Cooldown { until: a_until }, CredentialStatus::Cooldown { until: b_until }) => {
            CredentialStatus::Cooldown {
                until: (*a_until).max(*b_until),
            }
        }
        (CredentialStatus::Cooldown { until }, CredentialStatus::Transient { until: other })
        | (CredentialStatus::Transient { until: other }, CredentialStatus::Cooldown { until }) => {
            CredentialStatus::Cooldown {
                until: (*until).max(*other),
            }
        }
        (CredentialStatus::Transient { until: a_until }, CredentialStatus::Transient { until: b_until }) => {
            CredentialStatus::Transient {
                until: (*a_until).max(*b_until),
            }
        }
        (CredentialStatus::Cooldown { until }, CredentialStatus::Active)
        | (CredentialStatus::Active, CredentialStatus::Cooldown { until }) => {
            CredentialStatus::Cooldown { until: *until }
        }
        (CredentialStatus::Transient { until }, CredentialStatus::Active)
        | (CredentialStatus::Active, CredentialStatus::Transient { until }) => {
            CredentialStatus::Transient { until: *until }
        }
        _ => CredentialStatus::Active,
    }
}

pub fn serialize_states_json(states: &CredentialStatusList) -> String {
    serde_json::to_string(&states.0).unwrap_or_else(|_| "[]".to_string())
}

pub fn deserialize_states_json(input: &str) -> CredentialStatusList {
    let parsed = serde_json::from_str::<Vec<ModelStatusTuple>>(input).unwrap_or_default();
    CredentialStatusList(parsed)
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum ScheduleAction {
    CredentialStatus {
        provider: ProviderKind,
        id: String,
        model: String,
    },
    UsageAnchor { provider: ProviderKind, view_name: String },
}

impl ScheduleAction {
    fn key(&self) -> String {
        match self {
            ScheduleAction::CredentialStatus { provider, id, model } => {
                format!("credential:{}:{}:{}", provider.as_str(), id, model)
            }
            ScheduleAction::UsageAnchor { provider, view_name } => {
                format!("usage:{}:{}", provider.as_str(), view_name)
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ScheduleEntry {
    until: i64,
    action: ScheduleAction,
}

impl Ord for ScheduleEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.until
            .cmp(&other.until)
            .then_with(|| self.action.key().cmp(&other.action.key()))
    }
}

impl PartialOrd for ScheduleEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct CredentialStatusScheduler {
    queue: Mutex<BinaryHeap<Reverse<ScheduleEntry>>>,
    notify: Notify,
}

impl Default for CredentialStatusScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStatusScheduler {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(BinaryHeap::new()),
            notify: Notify::new(),
        }
    }

    pub async fn schedule(&self, provider: ProviderKind, id: String, model: String, until: i64) {
        let mut queue = self.queue.lock().await;
        queue.push(Reverse(ScheduleEntry {
            until,
            action: ScheduleAction::CredentialStatus { provider, id, model },
        }));
        drop(queue);
        self.notify.notify_one();
    }

    pub async fn schedule_usage_anchor(
        &self,
        provider: ProviderKind,
        view_name: String,
        until: i64,
    ) {
        let mut queue = self.queue.lock().await;
        queue.push(Reverse(ScheduleEntry {
            until,
            action: ScheduleAction::UsageAnchor { provider, view_name },
        }));
        drop(queue);
        self.notify.notify_one();
    }

    pub fn start(self: Arc<Self>, ctx: Arc<AppContext>) {
        tokio::spawn(async move {
            self.run(ctx).await;
        });
    }

    async fn run(self: Arc<Self>, ctx: Arc<AppContext>) {
        loop {
            let next = {
                let mut queue = self.queue.lock().await;
                queue.pop()
            };
            let Some(Reverse(entry)) = next else {
                self.notify.notified().await;
                continue;
            };

            let now = now_timestamp();
            if entry.until > now {
                let wait = (entry.until - now) as u64;
                tokio::select! {
                    _ = sleep(Duration::from_secs(wait)) => {}
                    _ = self.notify.notified() => {
                        let mut queue = self.queue.lock().await;
                        queue.push(Reverse(entry));
                        continue;
                    }
                }
            }

            match entry.action {
                ScheduleAction::CredentialStatus { provider, id, model } => {
                    let _ = ctx
                        .update_credential_status_by_id(provider, &id, &model, |status, now| {
                            match status {
                                CredentialStatus::Cooldown { until }
                                | CredentialStatus::Transient { until }
                                    if *until <= now =>
                                {
                                    Some(CredentialStatus::Active)
                                }
                                _ => None,
                            }
                        })
                        .await;
                }
                ScheduleAction::UsageAnchor { provider, view_name } => {
                    if let Some(slot_secs) = slot_secs_for_view(provider, &view_name)
                        && slot_secs > 0
                    {
                        let now = now_timestamp();
                        let current_anchor = slot_start(entry.until, slot_secs, now);
                        let _ = ctx
                            .usage_store()
                            .set_anchor(provider, &view_name, current_anchor)
                            .await;
                        let next_until = next_anchor_ts(current_anchor, slot_secs, now);
                        self.schedule_usage_anchor(provider, view_name, next_until)
                            .await;
                    }
                }
            }
        }
    }
}

pub fn now_timestamp() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

pub fn status_from_error(prev: &CredentialStatus, now: i64) -> CredentialStatus {
    let until = next_backoff_until(prev, now);
    CredentialStatus::Transient { until }
}

pub fn status_from_response(
    prev: &CredentialStatus,
    status: StatusCode,
    headers: &HeaderMap,
    now: i64,
) -> CredentialStatus {
    if status.is_success() {
        return CredentialStatus::Active;
    }
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return CredentialStatus::Disabled;
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        let until = match parse_retry_after(headers, now) {
            Some(until) => clamp_until(now, until),
            None => next_backoff_until(prev, now),
        };
        return CredentialStatus::Cooldown { until };
    }
    if status.is_server_error() {
        let until = next_backoff_until(prev, now);
        return CredentialStatus::Transient { until };
    }
    prev.clone()
}

fn parse_retry_after(headers: &HeaderMap, now: i64) -> Option<i64> {
    if let Some(value) = headers.get("retry-after")
        && let Ok(text) = value.to_str()
            && let Some(until) = parse_until_value(text, now) {
                return Some(until);
            }
    for name in [
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
        "x-ratelimit-reset",
    ] {
        if let Some(value) = headers.get(name)
            && let Ok(text) = value.to_str()
                && let Some(until) = parse_until_value(text, now) {
                    return Some(until);
                }
    }
    None
}

fn parse_until_value(text: &str, now: i64) -> Option<i64> {
    let value = text.trim().parse::<i64>().ok()?;
    if value <= 0 {
        return None;
    }
    if value > now {
        return Some(value);
    }
    Some(now + value)
}

fn clamp_until(now: i64, until: i64) -> i64 {
    let max_until = now + MAX_BACKOFF_SECS;
    if until > max_until {
        max_until
    } else {
        until
    }
}

fn next_backoff_until(prev: &CredentialStatus, now: i64) -> i64 {
    let previous = match prev {
        CredentialStatus::Cooldown { until }
        | CredentialStatus::Transient { until } => {
            if *until > now {
                *until - now
            } else {
                BASE_BACKOFF_SECS
            }
        }
        _ => BASE_BACKOFF_SECS,
    };
    let next = previous.saturating_mul(2).clamp(BASE_BACKOFF_SECS, MAX_BACKOFF_SECS);
    now + next
}
