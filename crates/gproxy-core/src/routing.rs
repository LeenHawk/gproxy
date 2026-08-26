use gproxy_protocol::{Operation, OperationKey, OperationKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RoutingImplementation {
    Passthrough,
    TransformTo,
    Local,
    Unsupported,
}

impl RoutingImplementation {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Passthrough => "passthrough",
            Self::TransformTo => "transform_to",
            Self::Local => "local",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Some(match value {
            "passthrough" => Self::Passthrough,
            "transform_to" => Self::TransformTo,
            "local" => Self::Local,
            "unsupported" => Self::Unsupported,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RoutingRuleSpec {
    pub id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: String,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledRoutingRule {
    pub operation: Operation,
    pub kind: OperationKind,
    pub implementation: RoutingImplementation,
    pub destination: Option<OperationKey>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingDecision {
    Passthrough,
    TransformTo(OperationKey),
    Local,
    Unsupported,
}

pub fn compile(spec: &RoutingRuleSpec) -> Result<CompiledRoutingRule, String> {
    let operation = Operation::from_id(&spec.operation)
        .ok_or_else(|| format!("unknown operation `{}`", spec.operation))?;
    let kind = OperationKind::from_id(&spec.kind)
        .ok_or_else(|| format!("unknown operation kind `{}`", spec.kind))?;
    let implementation = RoutingImplementation::from_id(&spec.implementation)
        .ok_or_else(|| format!("unknown routing implementation `{}`", spec.implementation))?;
    let destination = match (&spec.dest_operation, &spec.dest_kind) {
        (Some(operation), Some(kind)) => Some(OperationKey {
            operation: Operation::from_id(operation)
                .ok_or_else(|| format!("unknown destination operation `{operation}`"))?,
            kind: OperationKind::from_id(kind)
                .ok_or_else(|| format!("unknown destination kind `{kind}`"))?,
        }),
        (None, None) => None,
        _ => return Err("destination operation and kind must be set together".into()),
    };
    if implementation == RoutingImplementation::TransformTo && destination.is_none() {
        return Err("transform_to requires a destination".into());
    }
    if implementation != RoutingImplementation::TransformTo && destination.is_some() {
        return Err("only transform_to accepts a destination".into());
    }
    Ok(CompiledRoutingRule {
        operation,
        kind,
        implementation,
        destination,
        sort_order: spec.sort_order,
    })
}

pub fn compile_all(specs: &[RoutingRuleSpec]) -> Result<Vec<CompiledRoutingRule>, String> {
    let mut rules = specs
        .iter()
        .filter(|spec| spec.enabled)
        .map(compile)
        .collect::<Result<Vec<_>, _>>()?;
    rules.sort_by_key(|rule| rule.sort_order);
    Ok(rules)
}

pub fn decide(rules: &[CompiledRoutingRule], source: OperationKey) -> Option<RoutingDecision> {
    let rule = rules
        .iter()
        .find(|rule| rule.operation == source.operation && rule.kind == source.kind)?;
    Some(match rule.implementation {
        RoutingImplementation::Passthrough => RoutingDecision::Passthrough,
        RoutingImplementation::TransformTo => {
            RoutingDecision::TransformTo(rule.destination.expect("compiled destination"))
        }
        RoutingImplementation::Local => RoutingDecision::Local,
        RoutingImplementation::Unsupported => RoutingDecision::Unsupported,
    })
}
