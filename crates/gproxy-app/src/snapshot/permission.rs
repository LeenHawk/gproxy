//! What a caller is allowed to ask for: provider, model and operation.
//!
//! A `permissions` row is a grant or a refusal for one subject over one
//! provider (or all), one model glob (or all) and one operation (or all).
//! There is no implicit grant: a caller with no applicable rule is refused.
//! That is the whole point of the table — adding a key must not silently add
//! access to every provider the instance has.

use crate::snapshot::glob;
use gproxy_protocol::Operation;
use gproxy_store::entity::identity::permission;
use std::collections::BTreeSet;

/// Who is asking. A gateway key is its own subject *and* its user's: a rule
/// written for the user applies to every key that user holds, and a rule
/// written for one key applies only to it.
#[derive(Clone, Copy, Debug)]
pub struct Subject<'a> {
    pub user_id: &'a str,
    /// None for a caller authenticated as a person (console, portal) rather
    /// than through a key. Key-scoped rules then never apply.
    pub api_key_id: Option<&'a str>,
}

impl<'a> Subject<'a> {
    pub fn user(user_id: &'a str) -> Self {
        Self {
            user_id,
            api_key_id: None,
        }
    }

    pub fn key(user_id: &'a str, api_key_id: &'a str) -> Self {
        Self {
            user_id,
            api_key_id: Some(api_key_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Carries why, for the operator's log and the caller's 403. A refusal
    /// that does not name its rule is unanswerable in support.
    Deny(String),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// One compiled rule. `None` in a scope column means "any".
#[derive(Clone, Debug)]
struct Rule {
    id: String,
    user_id: Option<String>,
    api_key_id: Option<String>,
    provider_id: Option<String>,
    /// None when the pattern is blank or `*`, which match a request with no
    /// model at all. Any other pattern requires one.
    model_pattern: Option<String>,
    operation: Option<String>,
    allow: bool,
}

/// The rules of one snapshot, pre-sorted into the order they are evaluated in.
#[derive(Clone, Debug, Default)]
pub struct PermissionSet {
    rules: Vec<Rule>,
}

impl PermissionSet {
    /// Compile and order the rules once, at assembly, so a decision is a
    /// linear scan with no allocation and no sorting on the request path.
    ///
    /// Rows that cannot mean anything are dropped and counted: an `action`
    /// that is neither `allow` nor `deny`, and a rule with neither a
    /// `user_id` nor an `api_key_id`, which would otherwise be a grant to
    /// nobody or, worse, read as a grant to everybody.
    pub fn build(rows: &[permission::Model]) -> Self {
        let mut rules = Vec::with_capacity(rows.len());
        let mut dropped = 0_usize;
        for row in rows {
            let allow = match row.action.trim().to_ascii_lowercase().as_str() {
                "allow" => true,
                "deny" => false,
                _ => {
                    dropped += 1;
                    continue;
                }
            };
            if row.user_id.is_none() && row.api_key_id.is_none() {
                dropped += 1;
                continue;
            }
            let pattern = row.model_pattern.trim();
            rules.push((
                row.priority,
                Rule {
                    id: row.id.clone(),
                    user_id: row.user_id.clone(),
                    api_key_id: row.api_key_id.clone(),
                    provider_id: row.provider_id.clone(),
                    model_pattern: (!pattern.is_empty() && pattern != "*")
                        .then(|| pattern.to_string()),
                    operation: row
                        .operation
                        .as_deref()
                        .map(str::trim)
                        .filter(|op| !op.is_empty())
                        .map(str::to_ascii_lowercase),
                    allow,
                },
            ));
        }
        if dropped > 0 {
            tracing::warn!(
                count = dropped,
                "permission rows skipped: unknown action, or no user and no api key subject"
            );
        }
        // Highest priority first, then by id so that equal priorities always
        // resolve the same way on every instance and after every reload.
        rules.sort_by(|(left, a), (right, b)| right.cmp(left).then_with(|| a.id.cmp(&b.id)));
        Self {
            rules: rules.into_iter().map(|(_, rule)| rule).collect(),
        }
    }

    /// Decide one request against the rules.
    ///
    /// Rules are scanned in `(priority DESC, id)` order and **the first
    /// applicable one decides**: the first `deny` reached wins, and reaching
    /// an `allow` first is what grants the request. Priority is therefore how
    /// an operator carves an exception out of a broad rule in either
    /// direction — `allow *` at 0 with `deny gpt-4*` at 10, or `deny *` at 0
    /// with `allow gpt-4o` at 10. Two applicable rules that disagree at the
    /// *same* priority are resolved by id, which is stable but arbitrary; give
    /// overlapping allow/deny rules distinct priorities.
    ///
    /// With no applicable rule at all the request is denied. There is no
    /// default grant.
    pub fn decide(
        &self,
        subject: &Subject<'_>,
        provider_id: &str,
        model: Option<&str>,
        operation: Operation,
    ) -> Decision {
        match self.deciding(subject, provider_id, model, operation) {
            Some(rule) if rule.allow => Decision::Allow,
            Some(rule) => Decision::Deny(format!("denied by permission rule `{}`", rule.id)),
            None => Decision::Deny("no permission grants this".into()),
        }
    }

    /// Whether `decide` would allow, without building the reason a refusal
    /// carries. For callers that ask once per provider and only keep a yes.
    pub fn allows(
        &self,
        subject: &Subject<'_>,
        provider_id: &str,
        model: Option<&str>,
        operation: Operation,
    ) -> bool {
        self.deciding(subject, provider_id, model, operation)
            .is_some_and(|rule| rule.allow)
    }

    /// Whether this subject may reach `provider_id`'s vendor services — the
    /// profile, usage, plugin and remote-control endpoints a coding agent
    /// calls that are not model traffic.
    ///
    /// A service names **no model and no operation**, and it is decided the
    /// way the table already decides a request without a model: a column
    /// narrower than "any" cannot describe it, so only rules whose model
    /// pattern is blank or `*` *and* whose operation is unset apply. The same
    /// first-applicable-rule order then decides, with the same default deny.
    ///
    /// The consequence is deliberate in both directions. An `allow` scoped to
    /// `list_models` (or any one operation) does not open a provider's
    /// account endpoints — that grant was written about one kind of model
    /// request, and reading it as "everything on this provider" would widen
    /// it behind the operator's back. A provider-wide `deny`, on the other
    /// hand, does close them: a service is still a request to that provider.
    /// Mapping services onto some existing [`Operation`] instead would make an
    /// operation-scoped rule silently govern endpoints it was never about.
    pub fn allows_service(&self, subject: &Subject<'_>, provider_id: &str) -> bool {
        self.rules
            .iter()
            .find(|rule| rule.applies(subject, provider_id, None, None))
            .is_some_and(|rule| rule.allow)
    }

    /// The first applicable rule in evaluation order, if any.
    fn deciding(
        &self,
        subject: &Subject<'_>,
        provider_id: &str,
        model: Option<&str>,
        operation: Operation,
    ) -> Option<&Rule> {
        let operation: &'static str = operation.into();
        self.rules
            .iter()
            .find(|rule| rule.applies(subject, provider_id, model, Some(operation)))
    }

    /// The providers of `all_providers` this subject may use for the request,
    /// which is the set a call hands to the sdk as `allowed_providers`. An
    /// empty set means the request is refused before any routing.
    pub fn allowed_providers<I, S>(
        &self,
        subject: &Subject<'_>,
        model: Option<&str>,
        operation: Operation,
        all_providers: I,
    ) -> BTreeSet<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        all_providers
            .into_iter()
            .filter(|provider| self.allows(subject, provider.as_ref(), model, operation))
            .map(|provider| provider.as_ref().to_string())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Rule {
    fn applies(
        &self,
        subject: &Subject<'_>,
        provider_id: &str,
        model: Option<&str>,
        // `None` for a request that names no operation (a vendor service):
        // only a rule that leaves the operation unset can describe it.
        operation: Option<&str>,
    ) -> bool {
        // Every subject column that is set must match. A well-formed row sets
        // exactly one; a row that names both a user and a key is the
        // intersection of the two, never their union.
        if let Some(user) = &self.user_id
            && user != subject.user_id
        {
            return false;
        }
        if let Some(key) = &self.api_key_id
            && subject.api_key_id != Some(key.as_str())
        {
            return false;
        }
        if let Some(provider) = &self.provider_id
            && provider != provider_id
        {
            return false;
        }
        if let Some(wanted) = &self.operation
            && operation != Some(wanted.as_str())
        {
            return false;
        }
        match &self.model_pattern {
            // Blank or `*`: any model, and also a request that names none.
            None => true,
            // A narrower pattern cannot describe a request without a model.
            Some(pattern) => model.is_some_and(|model| glob::matches(pattern, model)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rule that grants alice everything, as a starting point the tests
    /// narrow one field at a time.
    fn rule(id: &str, action: &str, priority: i32) -> permission::Model {
        permission::Model {
            id: id.into(),
            user_id: Some("alice".into()),
            api_key_id: None,
            provider_id: None,
            model_pattern: "*".into(),
            action: action.into(),
            operation: None,
            priority,
        }
    }

    /// Narrowing combinators, so a test reads as the rule it is about.
    trait Narrow: Sized {
        fn subject(self, user: Option<&str>, key: Option<&str>) -> Self;
        fn on_provider(self, provider: &str) -> Self;
        fn on_model(self, pattern: &str) -> Self;
        fn on_operation(self, operation: &str) -> Self;
    }

    impl Narrow for permission::Model {
        fn subject(mut self, user: Option<&str>, key: Option<&str>) -> Self {
            self.user_id = user.map(Into::into);
            self.api_key_id = key.map(Into::into);
            self
        }
        fn on_provider(mut self, provider: &str) -> Self {
            self.provider_id = Some(provider.into());
            self
        }
        fn on_model(mut self, pattern: &str) -> Self {
            self.model_pattern = pattern.into();
            self
        }
        fn on_operation(mut self, operation: &str) -> Self {
            self.operation = Some(operation.into());
            self
        }
    }

    const GENERATE: Operation = Operation::GenerateContent;

    fn alice() -> Subject<'static> {
        Subject::key("alice", "k1")
    }

    #[test]
    fn no_rule_at_all_denies() {
        let set = PermissionSet::build(&[]);
        assert!(set.is_empty());
        let decision = set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE);
        assert_eq!(decision, Decision::Deny("no permission grants this".into()));
    }

    #[test]
    fn a_rule_for_another_subject_does_not_grant_anything() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).subject(Some("bob"), None)]);
        assert!(
            !set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_user_rule_covers_every_key_of_that_user() {
        let set = PermissionSet::build(&[rule("r", "allow", 0)]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert!(
            set.decide(&Subject::key("alice", "k2"), "openai", None, GENERATE)
                .is_allowed()
        );
        assert!(
            set.decide(&Subject::user("alice"), "openai", None, GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_key_rule_covers_only_that_key() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).subject(None, Some("k1"))]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&Subject::key("alice", "k2"), "openai", None, GENERATE)
                .is_allowed()
        );
        // A person signed in without a key is not covered by a key rule.
        assert!(
            !set.decide(&Subject::user("alice"), "openai", None, GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_rule_naming_both_a_user_and_a_key_is_their_intersection() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).subject(Some("alice"), Some("k1"))]);
        assert!(set.decide(&alice(), "openai", None, GENERATE).is_allowed());
        assert!(
            !set.decide(&Subject::key("alice", "k2"), "openai", None, GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&Subject::key("bob", "k1"), "openai", None, GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_provider_scoped_rule_does_not_reach_another_provider() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).on_provider("openai")]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "anthropic", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn an_operation_scoped_rule_does_not_reach_another_operation() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).on_operation("generate_content")]);
        assert!(
            set.decide(&alice(), "openai", Some("m"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "openai", Some("m"), Operation::ListModels)
                .is_allowed()
        );
        // An operation string no `Operation` spells can never apply.
        let unknown = PermissionSet::build(&[rule("r", "allow", 0).on_operation("teleport")]);
        assert!(
            !unknown
                .decide(&alice(), "openai", Some("m"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_blank_or_star_pattern_also_covers_a_request_with_no_model() {
        for pattern in ["", "   ", "*"] {
            let set = PermissionSet::build(&[rule("r", "allow", 0).on_model(pattern)]);
            assert!(
                set.decide(&alice(), "openai", None, Operation::ListModels)
                    .is_allowed(),
                "pattern {pattern:?}"
            );
            assert!(
                set.decide(&alice(), "openai", Some("anything"), GENERATE)
                    .is_allowed()
            );
        }
    }

    #[test]
    fn a_narrower_pattern_needs_a_model_to_match() {
        let set = PermissionSet::build(&[rule("r", "allow", 0).on_model("gpt-*")]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "openai", Some("claude-3"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "openai", None, Operation::ListModels)
                .is_allowed()
        );
    }

    #[test]
    fn globs_follow_the_same_grammar_as_the_engines() {
        let set = PermissionSet::build(&[
            rule("exact", "allow", 0).on_model("gpt-4o"),
            rule("single", "allow", 0).on_model("o?-mini"),
        ]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "openai", Some("gpt-4o-mini"), GENERATE)
                .is_allowed()
        );
        assert!(
            set.decide(&alice(), "openai", Some("o3-mini"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "openai", Some("o3x-mini"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn a_deny_carves_an_exception_out_of_a_broad_allow() {
        let set = PermissionSet::build(&[
            rule("allow-all", "allow", 0),
            rule("deny-gpt", "deny", 10).on_model("gpt-*"),
        ]);
        assert!(
            set.decide(&alice(), "openai", Some("claude-3"), GENERATE)
                .is_allowed()
        );
        assert_eq!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE),
            Decision::Deny("denied by permission rule `deny-gpt`".into())
        );
    }

    #[test]
    fn an_allow_carves_an_exception_out_of_a_broad_deny() {
        let set = PermissionSet::build(&[
            rule("deny-all", "deny", 0),
            rule("allow-one", "allow", 10).on_model("gpt-4o"),
        ]);
        assert!(
            set.decide(&alice(), "openai", Some("gpt-4o"), GENERATE)
                .is_allowed()
        );
        assert_eq!(
            set.decide(&alice(), "openai", Some("gpt-4o-mini"), GENERATE),
            Decision::Deny("denied by permission rule `deny-all`".into())
        );
    }

    #[test]
    fn equal_priorities_resolve_by_id_and_always_the_same_way() {
        let deny_first =
            PermissionSet::build(&[rule("b-allow", "allow", 5), rule("a-deny", "deny", 5)]);
        assert_eq!(
            deny_first.decide(&alice(), "openai", Some("m"), GENERATE),
            Decision::Deny("denied by permission rule `a-deny`".into())
        );
        // Row order does not matter; only (priority, id) does.
        let reversed =
            PermissionSet::build(&[rule("a-deny", "deny", 5), rule("b-allow", "allow", 5)]);
        assert_eq!(
            reversed.decide(&alice(), "openai", Some("m"), GENERATE),
            deny_first.decide(&alice(), "openai", Some("m"), GENERATE)
        );
    }

    #[test]
    fn an_inapplicable_deny_never_shadows_a_lower_priority_allow() {
        let set = PermissionSet::build(&[
            rule("deny-other", "deny", 100).on_provider("anthropic"),
            rule("allow-openai", "allow", 0).on_provider("openai"),
        ]);
        assert!(
            set.decide(&alice(), "openai", Some("m"), GENERATE)
                .is_allowed()
        );
        assert!(
            !set.decide(&alice(), "anthropic", Some("m"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn unusable_rows_are_dropped_rather_than_trusted() {
        let set = PermissionSet::build(&[
            rule("bad-action", "maybe", 0),
            rule("no-subject", "allow", 0).subject(None, None),
        ]);
        assert!(set.is_empty());
        assert!(
            !set.decide(&alice(), "openai", Some("m"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn the_action_is_read_case_insensitively() {
        let set = PermissionSet::build(&[rule("r", " ALLOW ", 0)]);
        assert!(
            set.decide(&alice(), "openai", Some("m"), GENERATE)
                .is_allowed()
        );
    }

    #[test]
    fn the_allowed_provider_set_is_the_decision_taken_per_provider() {
        let set = PermissionSet::build(&[
            rule("a", "allow", 0).on_provider("openai"),
            rule("b", "allow", 0)
                .on_provider("anthropic")
                .on_model("gpt-*"),
            rule("c", "deny", 0).on_provider("google"),
        ]);
        let providers = ["openai", "anthropic", "google", "unlisted"];
        assert_eq!(
            set.allowed_providers(&alice(), Some("gpt-4o"), GENERATE, providers),
            BTreeSet::from(["anthropic".to_string(), "openai".to_string()])
        );
        assert_eq!(
            set.allowed_providers(&alice(), Some("claude-3"), GENERATE, providers),
            BTreeSet::from(["openai".to_string()])
        );
        assert!(
            PermissionSet::default()
                .allowed_providers(&alice(), None, GENERATE, providers)
                .is_empty()
        );
    }

    #[test]
    fn a_service_is_reached_only_through_a_provider_wide_rule() {
        // A provider-wide grant opens the provider's services.
        let set = PermissionSet::build(&[rule("r", "allow", 0).on_provider("openai")]);
        assert!(set.allows_service(&alice(), "openai"));
        assert!(!set.allows_service(&alice(), "anthropic"));

        // A grant narrowed to a model or to an operation was written about
        // model traffic and does not describe a request that names neither.
        for narrowed in [
            rule("r", "allow", 0).on_model("gpt-*"),
            rule("r", "allow", 0).on_operation("list_models"),
        ] {
            let set = PermissionSet::build(&[narrowed]);
            assert!(!set.allows_service(&alice(), "openai"));
        }

        // A provider-wide deny closes the services too, in priority order.
        let set = PermissionSet::build(&[
            rule("a", "allow", 0),
            rule("d", "deny", 10).on_provider("openai"),
            // An operation-scoped deny is not about services at all.
            rule("g", "deny", 20).on_operation("generate_content"),
        ]);
        assert!(!set.allows_service(&alice(), "openai"));
        assert!(set.allows_service(&alice(), "anthropic"));
        assert!(!PermissionSet::default().allows_service(&alice(), "openai"));
    }
}
