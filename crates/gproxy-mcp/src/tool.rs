//! Tool names as callers see them, and what a tool does to the world.

use rmcp::model::ToolAnnotations;
use serde::{Deserialize, Serialize};

/// Between the server and the tool in a name on the aggregated endpoint.
pub const SEPARATOR: &str = "__";
/// What model APIs accept for a function name.
const MAX_NAME: usize = 64;

/// `{server}__{tool}`, or `None` when the result is not a name every client
/// accepts: longer than 64 characters or outside `[A-Za-z0-9_-]`.
pub fn exposed_name(server: &str, tool: &str) -> Option<String> {
    let name = format!("{server}{SEPARATOR}{tool}");
    (name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    .then_some(name)
}

/// The server and its tool behind an exposed name. The servers are given
/// because a server's own name may contain the separator.
pub fn resolve<'a>(
    exposed: &'a str,
    servers: impl IntoIterator<Item = &'a str>,
) -> Option<(&'a str, &'a str)> {
    servers.into_iter().find_map(|server| {
        let tool = exposed.strip_prefix(server)?.strip_prefix(SEPARATOR)?;
        (!tool.is_empty()).then_some((server, tool))
    })
}

/// What calling a tool can do, from least to most consequential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Nature {
    /// Changes nothing.
    Read,
    /// Changes things, and only by adding to them.
    Write,
    /// May delete or overwrite.
    Destructive,
}

impl Nature {
    pub const ALL: [Self; 3] = [Self::Read, Self::Write, Self::Destructive];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|nature| nature.as_str() == text)
    }

    /// What the server's annotations say, read with the specification's
    /// defaults: a tool is not read-only unless marked so, and destructive
    /// unless marked otherwise.
    pub fn declared(annotations: Option<&ToolAnnotations>) -> Self {
        let hint = |pick: fn(&ToolAnnotations) -> Option<bool>| annotations.and_then(pick);
        if hint(|a| a.read_only_hint) == Some(true) {
            Self::Read
        } else if hint(|a| a.destructive_hint) == Some(false) {
            Self::Write
        } else {
            Self::Destructive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotations(read_only: Option<bool>, destructive: Option<bool>) -> ToolAnnotations {
        ToolAnnotations::from_raw(None, read_only, destructive, None, None)
    }

    #[test]
    fn an_unmarked_tool_is_destructive() {
        assert_eq!(Nature::declared(None), Nature::Destructive);
        assert_eq!(
            Nature::declared(Some(&annotations(None, None))),
            Nature::Destructive
        );
        assert_eq!(
            Nature::declared(Some(&annotations(Some(false), None))),
            Nature::Destructive
        );
        assert_eq!(
            Nature::declared(Some(&annotations(None, Some(false)))),
            Nature::Write
        );
        // Read-only wins: the destructive hint only means something for a tool that writes.
        assert_eq!(
            Nature::declared(Some(&annotations(Some(true), Some(true)))),
            Nature::Read
        );
    }

    #[test]
    fn names_no_client_would_accept_are_not_exposed() {
        assert_eq!(
            exposed_name("wiki", "ask_question").as_deref(),
            Some("wiki__ask_question")
        );
        assert_eq!(exposed_name("wiki", "ask.question"), None);
        assert_eq!(exposed_name("my wiki", "ask"), None);
        assert_eq!(
            exposed_name("wiki", &"a".repeat(58)).map(|n| n.len()),
            Some(64)
        );
        assert_eq!(exposed_name("wiki", &"a".repeat(59)), None);
    }

    #[test]
    fn an_exposed_name_resolves_against_the_servers_that_exist() {
        let servers = ["a", "a__b"];
        assert_eq!(resolve("a__b__c", servers), Some(("a", "b__c")));
        assert_eq!(resolve("a__b__c", ["a__b"]), Some(("a__b", "c")));
        assert_eq!(resolve("x__c", servers), None);
        assert_eq!(resolve("a__", servers), None);
    }
}
