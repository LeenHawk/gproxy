//! `*`-wildcard glob matching (§8-B2). The only metachar is `*`; matching is
//! anchored at both ends. Used by process rules, tokenizer map look-ups, and
//! authz route-permission checks.

/// Returns `true` iff `pattern` glob-matches `value`.
///
/// `*` matches zero or more of any character. No other metacharacters.
/// Anchored at both ends: `"sonnet"` does **not** match `"claude-sonnet"`.
pub fn matches(pattern: &str, value: &str) -> bool {
    fn inner(p: &[u8], v: &[u8]) -> bool {
        match p.split_first() {
            None => v.is_empty(),
            Some((b'*', rest)) => (0..=v.len()).any(|i| inner(rest, &v[i..])),
            Some((c, rest)) => v
                .split_first()
                .is_some_and(|(vc, vrest)| vc == c && inner(rest, vrest)),
        }
    }
    inner(pattern.as_bytes(), value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn glob_semantics() {
        assert!(matches("*", "anything"));
        assert!(matches("claude-*", "claude-sonnet-4"));
        assert!(matches("*sonnet*", "claude-sonnet-4"));
        assert!(!matches("claude-*", "gpt-4"));
        assert!(!matches("sonnet", "claude-sonnet")); // anchored
        assert!(matches("", ""));
    }
}
