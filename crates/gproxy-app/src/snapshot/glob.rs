//! The model-name glob shared by permission rules.
//!
//! `gproxy-core` compiles the same syntax to a regex in `rewrite::compile`
//! (`*` → `.*`, `?` → `.`, anchored, case-sensitive) for budgets, price rules
//! and rewrite filters, but that function is `pub(crate)` there. Rather than
//! pull `regex` into this crate for one pattern per rule, the same grammar is
//! matched directly here. The two must agree; the tests below encode what
//! `glob_to_regex` produces.

/// Whether `value` matches `pattern`. The match is anchored at both ends and
/// case-sensitive, `*` stands for any run of characters including none, and
/// `?` for exactly one. Characters are compared as `char`s, so a multi-byte
/// model name is never split in the middle.
pub fn matches(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    let (mut p, mut v) = (0, 0);
    // The most recent `*` and how much of `value` it had consumed, so a failed
    // match backtracks to it instead of rejecting the whole pattern.
    let mut star: Option<usize> = None;
    let mut consumed = 0;
    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            consumed = v;
            p += 1;
        } else if let Some(at) = star {
            consumed += 1;
            v = consumed;
            p = at + 1;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|ch| *ch == '*')
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn an_exact_pattern_matches_only_itself() {
        assert!(matches("gpt-4o", "gpt-4o"));
        assert!(!matches("gpt-4o", "gpt-4o-mini"));
        assert!(!matches("gpt-4o", "GPT-4O"));
        assert!(!matches("gpt-4o", ""));
    }

    #[test]
    fn a_star_stands_for_any_run_including_none() {
        assert!(matches("*", ""));
        assert!(matches("*", "anything"));
        assert!(matches("gpt-*", "gpt-4o"));
        assert!(matches("gpt-*", "gpt-"));
        assert!(!matches("gpt-*", "claude-3"));
        assert!(matches("*-mini", "gpt-4o-mini"));
        assert!(matches("*4o*", "gpt-4o-mini"));
        assert!(matches("**", "gpt-4o"));
    }

    #[test]
    fn a_question_mark_stands_for_exactly_one() {
        assert!(matches("gpt-?", "gpt-4"));
        assert!(!matches("gpt-?", "gpt-4o"));
        assert!(!matches("gpt-?", "gpt-"));
        assert!(matches("?", "x"));
    }

    #[test]
    fn backtracking_finds_a_match_a_greedy_scan_would_miss() {
        assert!(matches("*ab", "aab"));
        assert!(matches("*a*b", "xxaxxb"));
        assert!(!matches("*a*b", "xxaxx"));
        assert!(matches("a*b*c", "abbbc"));
    }

    #[test]
    fn multi_byte_names_are_compared_by_character() {
        assert!(matches("模型-?", "模型-一"));
        assert!(!matches("模型-?", "模型-一二"));
        assert!(matches("*型*", "模型-一"));
    }
}
