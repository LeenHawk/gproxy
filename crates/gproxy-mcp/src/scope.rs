//! Which tools a caller may use.
//!
//! A scope is `mcp:<server>:<tool>`. Either part may be `*`, `mcp:*` is
//! everything, and the tool part may name a nature instead: `@read`,
//! `@write`, `@destructive`.
//!
//! MCP is allowed by default: a subject with no rule may use everything it
//! has a credential for. Once a subject has a rule, the first that matches
//! decides, highest priority first, and a tool no rule matches is refused.

use std::str::FromStr;

use crate::tool::Nature;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Any,
    Tool(String),
    Nature(Nature),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// `None` is `*`.
    server: Option<String>,
    target: Target,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScopeError {
    #[error(
        "a scope is `mcp:<server>:<tool>`, `mcp:<server>:@read|@write|@destructive` or `mcp:*`"
    )]
    Syntax,
    #[error("unknown tool nature `@{0}`; use @read, @write or @destructive")]
    Nature(String),
}

impl FromStr for Scope {
    type Err = ScopeError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let rest = text.strip_prefix("mcp:").ok_or(ScopeError::Syntax)?;
        if rest == "*" {
            return Ok(Self {
                server: None,
                target: Target::Any,
            });
        }
        let (server, tool) = rest.split_once(':').ok_or(ScopeError::Syntax)?;
        if server.is_empty() || tool.is_empty() {
            return Err(ScopeError::Syntax);
        }
        let target = match (tool, tool.strip_prefix('@')) {
            ("*", _) => Target::Any,
            (_, Some(nature)) => Target::Nature(
                Nature::parse(nature).ok_or_else(|| ScopeError::Nature(nature.to_owned()))?,
            ),
            _ => Target::Tool(tool.to_owned()),
        };
        Ok(Self {
            server: (server != "*").then(|| server.to_owned()),
            target,
        })
    }
}

impl Scope {
    fn covers(&self, server: &str, tool: &str, nature: Nature) -> bool {
        self.server.as_deref().is_none_or(|named| named == server)
            && match &self.target {
                Target::Any => true,
                Target::Tool(named) => named == tool,
                Target::Nature(named) => *named == nature,
            }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub scope: Scope,
    pub allow: bool,
    pub priority: i32,
}

/// The rules of one subject, in the order they are tried.
#[derive(Clone, Debug, Default)]
pub struct Rules(Vec<Rule>);

impl Rules {
    /// Highest priority first; at one priority a refusal comes before a
    /// grant, then the id decides so every instance agrees.
    pub fn new(mut rules: Vec<Rule>) -> Self {
        rules.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.allow.cmp(&b.allow))
                .then_with(|| a.id.cmp(&b.id))
        });
        Self(rules)
    }

    pub fn allows(&self, server: &str, tool: &str, nature: Nature) -> bool {
        self.0.is_empty()
            || self
                .0
                .iter()
                .find(|rule| rule.scope.covers(server, tool, nature))
                .is_some_and(|rule| rule.allow)
    }
}

/// A key can only narrow what its user may do: both have to allow.
pub fn allows(user: &Rules, key: Option<&Rules>, server: &str, tool: &str, nature: Nature) -> bool {
    user.allows(server, tool, nature) && key.is_none_or(|key| key.allows(server, tool, nature))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ToolAnnotations;

    fn rules(list: &[(&str, bool, i32)]) -> Rules {
        Rules::new(
            list.iter()
                .enumerate()
                .map(|(index, (scope, allow, priority))| Rule {
                    id: index.to_string(),
                    scope: scope.parse().unwrap(),
                    allow: *allow,
                    priority: *priority,
                })
                .collect(),
        )
    }

    #[test]
    fn a_subject_without_rules_may_use_everything() {
        assert!(Rules::default().allows("wiki", "ask", Nature::Destructive));
        assert!(allows(
            &Rules::default(),
            None,
            "wiki",
            "ask",
            Nature::Destructive
        ));
    }

    #[test]
    fn one_rule_makes_everything_else_refused() {
        let user = rules(&[("mcp:wiki:ask", true, 0)]);
        assert!(user.allows("wiki", "ask", Nature::Read));
        assert!(!user.allows("wiki", "read_structure", Nature::Read));
        assert!(!user.allows("notion", "ask", Nature::Read));
    }

    #[test]
    fn wildcards_stand_for_any_server_or_any_tool() {
        assert!(rules(&[("mcp:*", true, 0)]).allows("wiki", "ask", Nature::Destructive));
        let any_tool = rules(&[("mcp:wiki:*", true, 0)]);
        assert!(any_tool.allows("wiki", "ask", Nature::Write));
        assert!(!any_tool.allows("notion", "ask", Nature::Write));
        let any_server = rules(&[("mcp:*:search", true, 0)]);
        assert!(any_server.allows("notion", "search", Nature::Read));
        assert!(!any_server.allows("notion", "create", Nature::Read));
    }

    #[test]
    fn a_nature_scope_follows_what_the_tool_does() {
        let user = rules(&[
            ("mcp:notion:@read", true, 0),
            ("mcp:notion:@write", true, 0),
        ]);
        assert!(user.allows("notion", "search", Nature::Read));
        assert!(user.allows("notion", "create_page", Nature::Write));
        assert!(!user.allows("notion", "delete_page", Nature::Destructive));
    }

    #[test]
    fn the_higher_priority_decides_and_a_tie_refuses() {
        let user = rules(&[("mcp:*", true, 0), ("mcp:wiki:ask", false, 10)]);
        assert!(!user.allows("wiki", "ask", Nature::Read));
        assert!(user.allows("wiki", "read", Nature::Read));
        let reversed = rules(&[("mcp:*", true, 10), ("mcp:wiki:ask", false, 0)]);
        assert!(reversed.allows("wiki", "ask", Nature::Read));
        let tie = rules(&[("mcp:*", true, 0), ("mcp:wiki:ask", false, 0)]);
        assert!(!tie.allows("wiki", "ask", Nature::Read));
    }

    #[test]
    fn a_key_only_narrows_its_user() {
        let user = rules(&[("mcp:wiki:*", true, 0)]);
        let key = rules(&[("mcp:*:@read", true, 0)]);
        assert!(allows(&user, Some(&key), "wiki", "ask", Nature::Read));
        assert!(!allows(&user, Some(&key), "wiki", "edit", Nature::Write));
        // What the key allows beyond its user stays refused.
        assert!(!allows(&user, Some(&key), "notion", "search", Nature::Read));
        // A key without rules adds nothing to its user's.
        assert!(!allows(
            &user,
            Some(&Rules::default()),
            "notion",
            "search",
            Nature::Read
        ));
    }

    #[test]
    fn an_override_replaces_the_declared_nature() {
        let user = rules(&[("mcp:wiki:@read", true, 0)]);
        let declared = Nature::declared(Some(&ToolAnnotations::new()));
        assert_eq!(declared, Nature::Destructive);
        for (administrator, allowed) in [(None, false), (Some(Nature::Read), true)] {
            let nature = administrator.unwrap_or(declared);
            assert_eq!(user.allows("wiki", "ask", nature), allowed);
        }
    }

    #[test]
    fn malformed_scopes_are_refused() {
        for text in ["wiki:ask", "mcp:", "mcp:wiki", "mcp::ask", "mcp:wiki:"] {
            assert_eq!(text.parse::<Scope>(), Err(ScopeError::Syntax), "{text}");
        }
        assert_eq!(
            "mcp:wiki:@admin".parse::<Scope>(),
            Err(ScopeError::Nature("admin".to_owned()))
        );
    }
}
