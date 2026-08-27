use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RoutingImplementationDto {
    Passthrough,
    TransformTo,
    Local,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RoutingRuleDto {
    pub id: i64,
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: RoutingImplementationDto,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RoutingRuleWriteRequest {
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: RoutingImplementationDto,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuleSetDto {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuleSetWriteRequest {
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum TextPositionDto {
    Prepend,
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RewriteActionDto {
    Set,
    Delete,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum HeaderModeDto {
    Override,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum TransformPhaseDto {
    Request,
    Response,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[ts(tag = "type", content = "value", rename_all = "snake_case")]
pub enum TransformLocateDto {
    Path(String),
    Paths(Vec<String>),
    Match(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(tag = "op", rename_all = "snake_case")]
pub enum TransformActionDto {
    ReplaceText { from: Option<String>, with: String },
    ReplaceRegex { pattern: String, with: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(tag = "kind", rename_all = "snake_case")]
pub enum RuleConfigDto {
    SystemText {
        text: String,
        position: TextPositionDto,
    },
    CacheBreakpoint {
        target: String,
        index: Option<i64>,
        ttl: Option<String>,
    },
    Rewrite {
        path: String,
        action: RewriteActionDto,
        #[ts(type = "unknown | null")]
        value: Option<Value>,
    },
    Transform {
        phase: TransformPhaseDto,
        locate: TransformLocateDto,
        actions: Vec<TransformActionDto>,
        limit: Option<usize>,
    },
    Header {
        name: String,
        value: String,
        mode: HeaderModeDto,
    },
}

impl RuleConfigDto {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SystemText { .. } => "system_text",
            Self::CacheBreakpoint { .. } => "cache_breakpoint",
            Self::Rewrite { .. } => "rewrite",
            Self::Transform { .. } => "transform",
            Self::Header { .. } => "header",
        }
    }

    pub fn storage(&self) -> Value {
        match self {
            Self::SystemText { text, position } => json!({"text": text, "position": position}),
            Self::CacheBreakpoint { target, index, ttl } => {
                json!({"target": target, "index": index, "ttl": ttl})
            }
            Self::Rewrite {
                path,
                action,
                value,
            } => json!({"path": path, "action": action, "value_json": value}),
            Self::Transform {
                phase,
                locate,
                actions,
                limit,
            } => {
                let locate = match locate {
                    TransformLocateDto::Path(value) => json!({"path": value}),
                    TransformLocateDto::Paths(value) => json!({"paths": value}),
                    TransformLocateDto::Match(value) => json!({"match": value}),
                };
                json!({"phase": phase, "locate": locate, "actions": actions, "limit": limit})
            }
            Self::Header { name, value, mode } => {
                json!({"name": name, "value": value, "mode": mode})
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RuleDto {
    pub id: i64,
    pub rule_set_id: i64,
    pub config: RuleConfigDto,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RuleWriteRequest {
    pub rule_set_id: i64,
    pub config: RuleConfigDto,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProviderRuleSetDto {
    pub id: i64,
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProviderRuleSetWriteRequest {
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum RulePresetCategoryDto {
    Application,
    Cache,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RulePresetRuleDto {
    pub config: RuleConfigDto,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RulePresetDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: RulePresetCategoryDto,
    pub rules: Vec<RulePresetRuleDto>,
}
