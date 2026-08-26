use http::HeaderName;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RuleKind {
    SystemText,
    CacheBreakpoint,
    Rewrite,
    Transform,
    Header,
}

impl RuleKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::SystemText => "system_text",
            Self::CacheBreakpoint => "cache_breakpoint",
            Self::Rewrite => "rewrite",
            Self::Transform => "transform",
            Self::Header => "header",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            Self::SystemText => 0,
            Self::CacheBreakpoint => 1,
            Self::Rewrite => 2,
            Self::Transform => 3,
            Self::Header => 4,
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "system_text" => Self::SystemText,
            "cache_breakpoint" => Self::CacheBreakpoint,
            "rewrite" => Self::Rewrite,
            "transform" => Self::Transform,
            "header" => Self::Header,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RuleSpec {
    pub id: i64,
    pub kind: String,
    pub config: Value,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextPosition {
    #[default]
    Prepend,
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewriteAction {
    Set,
    Delete,
    Merge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HeaderMode {
    #[default]
    Override,
    Merge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransformPhase {
    #[default]
    Request,
    Response,
    Both,
}

impl TransformPhase {
    pub const fn request(self) -> bool {
        matches!(self, Self::Request | Self::Both)
    }

    pub const fn response(self) -> bool {
        matches!(self, Self::Response | Self::Both)
    }
}

#[derive(Debug, Clone)]
pub struct CacheBreakpointConfig {
    pub target: String,
    pub index: Option<i64>,
    pub ttl: Option<String>,
}

#[derive(Debug, Clone)]
pub enum TransformLocate {
    Path(String),
    Paths(Vec<String>),
    Match(Regex),
}

#[derive(Debug, Clone)]
pub enum TransformAction {
    ReplaceText { from: Option<String>, with: String },
    ReplaceRegex { regex: Regex, with: String },
}

#[derive(Debug, Clone)]
pub struct TransformConfig {
    pub phase: TransformPhase,
    pub locate: TransformLocate,
    pub actions: Vec<TransformAction>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RuleConfig {
    SystemText {
        text: String,
        position: TextPosition,
    },
    CacheBreakpoint(CacheBreakpointConfig),
    Rewrite {
        path: String,
        action: RewriteAction,
        value: Option<Value>,
    },
    Transform(TransformConfig),
    Header {
        name: HeaderName,
        value: String,
        mode: HeaderMode,
    },
}

impl RuleConfig {
    pub const fn kind(&self) -> RuleKind {
        match self {
            Self::SystemText { .. } => RuleKind::SystemText,
            Self::CacheBreakpoint(_) => RuleKind::CacheBreakpoint,
            Self::Rewrite { .. } => RuleKind::Rewrite,
            Self::Transform(_) => RuleKind::Transform,
            Self::Header { .. } => RuleKind::Header,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub id: i64,
    pub config: RuleConfig,
    pub(crate) model_pattern: Option<Regex>,
    pub(crate) operations: Option<Vec<gproxy_protocol::Operation>>,
    pub(crate) header_pattern: Option<Regex>,
    pub(crate) sort_order: i64,
}
