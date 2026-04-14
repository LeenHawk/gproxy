use serde::{Deserialize, Serialize};

/// A rate-limit rule scoped to a model pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRule {
    /// The model selector for the rule.
    pub model_pattern: String,
    /// The allowed requests per minute, if present.
    pub rpm: Option<i32>,
    /// The allowed requests per day, if present.
    pub rpd: Option<i32>,
    /// The maximum total tokens declared for a single request, if present.
    pub total_tokens: Option<i64>,
}

/// Finds the most specific matching rule using exact, then longest-prefix, then `*` priority.
pub fn find_matching_rule<'a>(
    rules: &'a [RateLimitRule],
    model: &str,
) -> Option<&'a RateLimitRule> {
    if let Some(rule) = rules.iter().find(|rule| rule.model_pattern == model) {
        return Some(rule);
    }
    let mut best_rule: Option<&RateLimitRule> = None;
    let mut best_prefix_len = 0;
    for rule in rules {
        if let Some(prefix) = rule.model_pattern.strip_suffix('*')
            && model.starts_with(prefix)
            && prefix.len() > best_prefix_len
        {
            best_rule = Some(rule);
            best_prefix_len = prefix.len();
        }
    }
    if best_rule.is_some() {
        return best_rule;
    }
    rules.iter().find(|rule| rule.model_pattern == "*")
}
