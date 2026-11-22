//! Which MCP tools a caller may use, and what the administrator says a tool
//! does. Compiled once per revision; the rules themselves are `gproxy-mcp`'s.

use std::collections::HashMap;

use gproxy_mcp::{
    scope::{self, Rule, Rules},
    tool::Nature,
};
use gproxy_store::entity::{identity::mcp_scope, upstream::mcp_tool_nature};

use crate::Caller;

#[derive(Clone, Debug, Default)]
pub struct McpPolicy {
    users: HashMap<String, Rules>,
    keys: HashMap<String, Rules>,
    /// (provider id, tool) → the nature and the id of the row that says so.
    natures: HashMap<(String, String), (Nature, String)>,
}

impl McpPolicy {
    /// The operations validate every row they write, so one that does not
    /// parse here was not written by them; it is skipped and counted.
    pub fn build(scopes: &[mcp_scope::Model], natures: &[mcp_tool_nature::Model]) -> Self {
        let mut users: HashMap<String, Vec<Rule>> = HashMap::new();
        let mut keys: HashMap<String, Vec<Rule>> = HashMap::new();
        let mut skipped = 0_usize;
        for row in scopes {
            let allow = match row.action.as_str() {
                "allow" => true,
                "deny" => false,
                _ => {
                    skipped += 1;
                    continue;
                }
            };
            let (Ok(scope), Some(subject)) = (
                row.scope.parse(),
                match (&row.user_id, &row.api_key_id) {
                    (Some(user), None) => Some(users.entry(user.clone())),
                    (None, Some(key)) => Some(keys.entry(key.clone())),
                    _ => None,
                },
            ) else {
                skipped += 1;
                continue;
            };
            subject.or_default().push(Rule {
                id: row.id.clone(),
                scope,
                allow,
                priority: row.priority,
            });
        }
        let natures = natures
            .iter()
            .filter_map(|row| {
                let nature = Nature::parse(&row.nature);
                skipped += usize::from(nature.is_none());
                Some((
                    (row.provider_id.clone(), row.tool.clone()),
                    (nature?, row.id.clone()),
                ))
            })
            .collect();
        if skipped > 0 {
            tracing::warn!(
                count = skipped,
                "MCP scope or nature rows skipped: not valid"
            );
        }
        let compile = |rules: HashMap<String, Vec<Rule>>| {
            rules
                .into_iter()
                .map(|(subject, rules)| (subject, Rules::new(rules)))
                .collect()
        };
        Self {
            users: compile(users),
            keys: compile(keys),
            natures,
        }
    }

    /// The administrator's override for one tool, with the id of its row.
    pub fn override_of(&self, provider_id: &str, tool: &str) -> Option<(Nature, &str)> {
        self.natures
            .get(&(provider_id.to_owned(), tool.to_owned()))
            .map(|(nature, id)| (*nature, id.as_str()))
    }

    /// The user's rules, then the key's: a key only narrows its user.
    pub fn allows(&self, caller: &Caller, server: &str, tool: &str, nature: Nature) -> bool {
        static NONE: std::sync::LazyLock<Rules> = std::sync::LazyLock::new(Rules::default);
        scope::allows(
            self.users.get(&caller.user_id).unwrap_or(&NONE),
            caller
                .api_key_id
                .as_ref()
                .and_then(|key| self.keys.get(key)),
            server,
            tool,
            nature,
        )
    }
}
