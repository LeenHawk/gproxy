use std::collections::HashMap;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// Maps (operation, protocol) pairs to routing strategies.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DispatchTable {
    routes: HashMap<RouteKey, RouteImplementation>,
}

/// A (operation, protocol) pair identifying a route.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RouteKey {
    pub operation: OperationFamily,
    pub protocol: ProtocolKind,
}

/// How to handle a particular (operation, protocol) pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteImplementation {
    /// Forward request as-is to upstream (same protocol).
    Passthrough,
    /// Transform the request to a different (operation, protocol) before sending.
    TransformTo { destination: RouteKey },
    /// Handle locally without contacting upstream.
    Local,
    /// Not supported — return 501.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DispatchTableDocument {
    #[serde(default)]
    pub rules: Vec<DispatchRuleDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRuleDocument {
    pub route: RouteKey,
    pub implementation: RouteImplementation,
}

#[derive(Debug, Error)]
pub enum DispatchTableError {
    #[error("invalid dispatch json: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("duplicate dispatch route: operation={operation}, protocol={protocol}")]
    DuplicateRoute {
        operation: OperationFamily,
        protocol: ProtocolKind,
    },
}

impl DispatchTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a route.
    pub fn set(&mut self, key: RouteKey, implementation: RouteImplementation) {
        self.routes.insert(key, implementation);
    }

    /// Look up how to handle a route.
    pub fn resolve(&self, key: &RouteKey) -> Option<&RouteImplementation> {
        self.routes.get(key)
    }

    /// Resolve a source key to its final (source, destination) pair,
    /// following TransformTo chains.
    pub fn resolve_destination(&self, src: &RouteKey) -> Option<RouteKey> {
        match self.routes.get(src)? {
            RouteImplementation::Passthrough => Some(src.clone()),
            RouteImplementation::TransformTo { destination } => Some(destination.clone()),
            RouteImplementation::Local | RouteImplementation::Unsupported => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn to_document(&self) -> DispatchTableDocument {
        let mut rules: Vec<_> = self
            .routes
            .iter()
            .map(|(route, implementation)| DispatchRuleDocument {
                route: route.clone(),
                implementation: implementation.clone(),
            })
            .collect();
        rules.sort_by(|left, right| {
            let left_key = (
                left.route.operation.to_string(),
                left.route.protocol.to_string(),
            );
            let right_key = (
                right.route.operation.to_string(),
                right.route.protocol.to_string(),
            );
            left_key.cmp(&right_key)
        });
        DispatchTableDocument { rules }
    }

    pub fn from_document(document: DispatchTableDocument) -> Result<Self, DispatchTableError> {
        let mut table = Self::new();
        let mut seen = HashSet::new();
        for rule in document.rules {
            let route = rule.route;
            if !seen.insert(route.clone()) {
                return Err(DispatchTableError::DuplicateRoute {
                    operation: route.operation,
                    protocol: route.protocol,
                });
            }
            table.set(route, rule.implementation);
        }
        Ok(table)
    }

    pub fn from_json_value(value: serde_json::Value) -> Result<Option<Self>, DispatchTableError> {
        let Some(document) = DispatchTableDocument::from_json_value(value)? else {
            return Ok(None);
        };
        Ok(Some(Self::from_document(document)?))
    }
}

impl DispatchTableDocument {
    pub fn from_json_value(value: serde_json::Value) -> Result<Option<Self>, DispatchTableError> {
        if value.is_null() {
            return Ok(None);
        }
        if value.as_object().is_some_and(|object| object.is_empty()) {
            return Ok(None);
        }
        let document: Self = serde_json::from_value(value)?;
        if document.rules.is_empty() {
            return Ok(None);
        }
        Ok(Some(document))
    }
}

impl RouteKey {
    pub const fn new(operation: OperationFamily, protocol: ProtocolKind) -> Self {
        Self {
            operation,
            protocol,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DispatchRuleDocument, DispatchTable, DispatchTableDocument, RouteImplementation, RouteKey,
    };
    use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

    #[test]
    fn dispatch_document_round_trips_through_dispatch_table() {
        let document = DispatchTableDocument {
            rules: vec![
                DispatchRuleDocument {
                    route: RouteKey::new(OperationFamily::CountToken, ProtocolKind::Claude),
                    implementation: RouteImplementation::TransformTo {
                        destination: RouteKey::new(
                            OperationFamily::CountToken,
                            ProtocolKind::OpenAi,
                        ),
                    },
                },
                DispatchRuleDocument {
                    route: RouteKey::new(OperationFamily::GeminiLive, ProtocolKind::Gemini),
                    implementation: RouteImplementation::Local,
                },
                DispatchRuleDocument {
                    route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::OpenAi),
                    implementation: RouteImplementation::Passthrough,
                },
                DispatchRuleDocument {
                    route: RouteKey::new(
                        OperationFamily::OpenAiResponseWebSocket,
                        ProtocolKind::OpenAi,
                    ),
                    implementation: RouteImplementation::Unsupported,
                },
            ],
        };

        let table = DispatchTable::from_document(document.clone()).expect("dispatch document");

        assert_eq!(table.to_document(), document);
    }

    #[test]
    fn dispatch_document_rejects_duplicate_source_routes() {
        let document = DispatchTableDocument {
            rules: vec![
                DispatchRuleDocument {
                    route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::OpenAi),
                    implementation: RouteImplementation::Passthrough,
                },
                DispatchRuleDocument {
                    route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::OpenAi),
                    implementation: RouteImplementation::Unsupported,
                },
            ],
        };

        let error = DispatchTable::from_document(document).expect_err("duplicate route");

        assert!(
            error.to_string().contains("duplicate"),
            "unexpected error: {error}"
        );
    }
}
