//! Small, compiled predicates. No configuration decoding on the request path.
use super::RewriteCompileError;
use crate::PathSegment;
use http::{HeaderMap, HeaderName};
use serde_json::Value;

enum Comparison<T = Value> {
    Eq(T),
    Ne(T),
    Exists,
    NotExists,
}
pub struct BodyCondition {
    matcher: BodyMatcher,
}
enum BodyMatcher {
    Fields {
        path: Vec<PathSegment>,
        comparison: Comparison,
    },
    Jmespath(jmespath::Expression<'static>),
}

/// The upstream generic `search(&Value)` converts the full tree every time.
/// Its public input trait lets us reuse one conversion without nightly specialization.
struct CachedDocument<'a>(&'a jmespath::Rcvar);
impl jmespath::ToJmespath for CachedDocument<'_> {
    fn to_jmespath(self) -> Result<jmespath::Rcvar, jmespath::JmespathError> {
        Ok(self.0.clone())
    }
}
pub struct HeaderCondition {
    name: HeaderName,
    comparison: Comparison<String>,
}

fn invalid(message: impl Into<String>) -> RewriteCompileError {
    RewriteCompileError::InvalidCondition(message.into())
}
fn decode<'a>(value: &'a Value, key: &str) -> Result<(&'a str, Comparison), RewriteCompileError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("condition must be an object"))?;
    if object
        .keys()
        .any(|k| ![key, "op", "value"].contains(&k.as_str()))
    {
        return Err(invalid("unknown condition field"));
    }
    let name = object
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("condition requires {key}")))?;
    let comparison = match object.get("op").and_then(Value::as_str) {
        Some("eq") => Comparison::Eq(
            object
                .get("value")
                .ok_or_else(|| invalid("eq requires value"))?
                .clone(),
        ),
        Some("ne") => Comparison::Ne(
            object
                .get("value")
                .ok_or_else(|| invalid("ne requires value"))?
                .clone(),
        ),
        Some(op @ ("exists" | "not_exists")) => {
            if object.contains_key("value") {
                return Err(invalid("existence conditions do not accept value"));
            }
            if op == "exists" {
                Comparison::Exists
            } else {
                Comparison::NotExists
            }
        }
        _ => return Err(invalid("op must be eq, ne, exists or not_exists")),
    };
    Ok((name, comparison))
}
impl BodyCondition {
    pub(super) fn compile(value: &Value) -> Result<Self, RewriteCompileError> {
        if let Some(source) = value.as_str() {
            if source.trim().is_empty() {
                return Err(invalid("JMESPath expression must not be empty"));
            }
            let expression =
                jmespath::compile(source).map_err(|error| invalid(format!("JMESPath: {error}")))?;
            return Ok(Self {
                matcher: BodyMatcher::Jmespath(expression),
            });
        }
        let (path, comparison) = decode(value, "path")?;
        let mut paths = super::compile::parse_paths(&serde_json::json!([path]))?;
        let path = paths.remove(0);
        if path.contains(&PathSegment::Wildcard) {
            return Err(invalid("condition paths do not support wildcards"));
        }
        Ok(Self {
            matcher: BodyMatcher::Fields { path, comparison },
        })
    }
    pub(super) fn matches(
        &self,
        document: &Value,
        cached: &mut Option<Result<jmespath::Rcvar, jmespath::JmespathError>>,
    ) -> bool {
        let (path, comparison) = match &self.matcher {
            BodyMatcher::Fields { path, comparison } => (path, comparison),
            BodyMatcher::Jmespath(expression) => {
                let data = cached.get_or_insert_with(|| {
                    jmespath::Variable::from_serializable(document).map(jmespath::Rcvar::new)
                });
                return data.as_ref().is_ok_and(|data| {
                    expression
                        .search(CachedDocument(data))
                        .is_ok_and(|result| result.as_boolean() == Some(true))
                });
            }
        };
        let value = path.iter().try_fold(document, |current, part| match part {
            PathSegment::Key(key) => current.as_object()?.get(key),
            PathSegment::Index(index) => current.as_array()?.get(*index),
            PathSegment::Wildcard => None,
        });
        match comparison {
            Comparison::Eq(expected) => value == Some(expected),
            Comparison::Ne(expected) => value.is_some_and(|v| v != expected),
            Comparison::Exists => value.is_some(),
            Comparison::NotExists => value.is_none(),
        }
    }
}
impl HeaderCondition {
    pub(super) fn compile(value: &Value) -> Result<Self, RewriteCompileError> {
        let (name, comparison) = decode(value, "name")?;
        let name = name
            .parse()
            .map_err(|_| invalid("invalid condition header name"))?;
        let comparison = match comparison {
            Comparison::Eq(Value::String(value)) => Comparison::Eq(value),
            Comparison::Ne(Value::String(value)) => Comparison::Ne(value),
            Comparison::Exists => Comparison::Exists,
            Comparison::NotExists => Comparison::NotExists,
            _ => return Err(invalid("header comparison value must be a string")),
        };
        Ok(Self { name, comparison })
    }
    pub(super) fn matches(&self, headers: &HeaderMap) -> bool {
        match &self.comparison {
            Comparison::Exists => headers.contains_key(&self.name),
            Comparison::NotExists => !headers.contains_key(&self.name),
            Comparison::Eq(expected) => headers
                .get_all(&self.name)
                .iter()
                .any(|v| v.as_bytes() == expected.as_bytes()),
            // Repeated headers: ne means present and none of the values equals it.
            Comparison::Ne(expected) => {
                headers.contains_key(&self.name)
                    && headers
                        .get_all(&self.name)
                        .iter()
                        .all(|v| v.as_bytes() != expected.as_bytes())
            }
        }
    }
}
