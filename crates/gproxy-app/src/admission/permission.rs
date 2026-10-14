//! Which providers a caller may reach for this request.
//!
//! Two filters, in this order.
//!
//! **OAuth scopes and the operation baseline** come first, because they concern the
//! *program* holding the token rather than the person who issued it. An
//! access token is a credential a user handed to somebody else's binary; until
//! an operator says otherwise, that binary may read the model catalogue, count
//! tokens, generate, stream and compact, and nothing else. Naming a client in
//! [`AppConfig.oauth.cli_client_ids`](crate::config::OAuthIssuerConfig) lifts
//! the baseline for that client. Every operation still requires its exact
//! `gproxy:<operation_id>` scope, including for those configured clients.
//!
//! **The permission rules** come second and produce the provider set, one
//! [`PermissionSet::decide`](crate::snapshot::PermissionSet::decide) per
//! provider. There is no implicit grant: a provider no rule reaches is not in
//! the set, and an empty set is a refusal before any routing happens.
//!
//! A vendor service call goes through the same two filters by way of
//! [`allow_service`], which adapts them to a request that names no model and
//! no operation.

use crate::{AppData, AppError, Caller, CallerKind};
use gproxy_protocol::Operation;
use std::collections::BTreeSet;

/// What an OAuth-grant caller may do when its client is not a known CLI
/// issuer: the coding-agent surface and nothing else.
///
/// The list is the operations a chat/coding client cannot work without.
/// Everything absent from it — embeddings, images, audio, files, video,
/// realtime, moderation — is either a different product a third-party client
/// has no reason to reach through somebody else's account, or an expensive
/// one. Adding a variant here widens access for tokens carrying that operation's
/// scope, so it is a deliberate decision and not a default.
const OAUTH_BASELINE: [Operation; 6] = [
    Operation::ListModels,
    Operation::GetModel,
    Operation::CountTokens,
    Operation::GenerateContent,
    Operation::StreamGenerateContent,
    Operation::CompactContent,
];

/// OAuth scope and baseline checks, for a host that needs to answer "may this
/// token do that?" without computing a provider set.
///
/// Applies to [`CallerKind::OAuthGrant`] only; every other caller passes
/// through. **An instance administrator does not bypass this one.** The
/// restriction protects the account holder from the client they authorized,
/// and an admin's account is exactly the one where that matters most: a token
/// issued to a third-party program must not become an administrative
/// credential because of who authorized it.
pub fn check_oauth_operation(
    caller: &Caller,
    operation: Operation,
    cli_client_ids: &[String],
) -> Result<(), AppError> {
    if caller.kind != CallerKind::OAuthGrant {
        return Ok(());
    }
    require_oauth_scope(caller, &format!("gproxy:{}", operation.id()))?;
    if known_cli_client(caller, cli_client_ids) || OAUTH_BASELINE.contains(&operation) {
        return Ok(());
    }
    Err(AppError::forbidden(
        "this operation is not available to OAuth clients",
    ))
}

/// Consent is an upper bound, including for operator-approved CLI clients.
/// Unknown/identity scopes never implicitly authorize a resource operation.
fn require_oauth_scope(caller: &Caller, required: &str) -> Result<(), AppError> {
    if caller.kind != CallerKind::OAuthGrant
        || caller
            .grant
            .as_ref()
            .is_some_and(|grant| grant.scopes.iter().any(|scope| scope == required))
    {
        return Ok(());
    }
    Err(AppError::forbidden(format!(
        "insufficient OAuth scope: {required} is required"
    )))
}

/// Whether an OAuth-grant caller's client is one the operator named in
/// `cli_client_ids`, which is what lifts the baseline for it.
fn known_cli_client(caller: &Caller, cli_client_ids: &[String]) -> bool {
    let client_id = caller
        .grant
        .as_ref()
        .map(|grant| grant.client_id.as_str())
        .unwrap_or_default();
    !client_id.is_empty() && cli_client_ids.iter().any(|id| id == client_id)
}

/// Whether this caller may reach `provider_id`'s vendor services at all —
/// the gate a service call passes before any credential is planned.
///
/// The same two filters as [`allowed_providers`], adapted to a request that
/// names no model and no [`Operation`]:
///
/// - **The OAuth baseline.** Vendor services — account profile, usage,
///   plugins, remote control — are not in [`OAUTH_BASELINE`]; they are the
///   account surface rather than the model surface a third-party client was
///   authorized for, and remote control in particular is not something a
///   program should acquire through somebody else's token by default. So an
///   OAuth-grant caller reaches them only when its client is named in
///   `cli_client_ids`, exactly as it would for any operation outside the
///   baseline. As there, an instance administrator's token gets no bypass.
/// - **The permission rules**, through
///   [`PermissionSet::allows_service`](crate::snapshot::PermissionSet::allows_service):
///   only a rule scoped to any model and any operation describes a service.
///   An instance administrator bypasses this filter for the reason given on
///   [`allowed_providers`].
///
/// Without this gate a service would be decided by credential visibility
/// alone, so a key whose rules refuse a provider could still read that
/// provider's account endpoints through any credential it happens to see.
pub fn allow_service(
    snapshot: &AppData,
    caller: &Caller,
    provider_id: &str,
    cli_client_ids: &[String],
) -> Result<(), AppError> {
    require_oauth_scope(caller, "gproxy:services")?;
    if caller.kind == CallerKind::OAuthGrant && !known_cli_client(caller, cli_client_ids) {
        return Err(AppError::forbidden(
            "vendor services are not available to OAuth clients",
        ));
    }
    if caller.is_instance_admin()
        || snapshot
            .permissions
            .allows_service(&caller.subject(), provider_id)
    {
        return Ok(());
    }
    Err(AppError::forbidden(
        "this provider's services are not permitted for this caller",
    ))
}

/// The providers of `all_providers` this request may use.
///
/// `cli_client_ids` is `AppConfig.oauth.cli_client_ids`; it is passed rather
/// than read from the snapshot because it is instance configuration, not a
/// row, and the two hosts supply it the same way.
///
/// **An instance administrator bypasses the permission filter entirely** and
/// receives `all_providers` unchanged. `users.role = "admin"` is the one role
/// that is not a membership: it already grants the operations that write the
/// `permissions` table, so a rule that refused an admin a provider would be a
/// rule they could delete. Making the bypass explicit keeps a mis-scoped deny
/// from locking the operator out of their own instance. It does **not** widen
/// credential visibility on its own — that is
/// [`super::credential`] — and it does not lift the OAuth baseline above.
pub fn allowed_providers(
    snapshot: &AppData,
    caller: &Caller,
    model: Option<&str>,
    operation: Operation,
    all_providers: &BTreeSet<String>,
    cli_client_ids: &[String],
) -> Result<BTreeSet<String>, AppError> {
    check_oauth_operation(caller, operation, cli_client_ids)?;
    let allowed = if caller.is_instance_admin() {
        all_providers.clone()
    } else {
        snapshot
            .permissions
            .allowed_providers(&caller.subject(), model, operation, all_providers)
    };
    if allowed.is_empty() {
        return Err(AppError::forbidden(
            "no provider is permitted for this model",
        ));
    }
    Ok(allowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GrantContext, admission::support::caller};
    use gproxy_store::{IdentityData, entity::identity::permission};

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn rule(id: &str, action: &str, provider: Option<&str>, pattern: &str) -> permission::Model {
        permission::Model {
            id: id.into(),
            user_id: Some("alice".into()),
            api_key_id: None,
            provider_id: provider.map(Into::into),
            model_pattern: pattern.into(),
            action: action.into(),
            operation: None,
            priority: 0,
        }
    }

    fn snapshot(rules: Vec<permission::Model>) -> AppData {
        let identity = IdentityData {
            permissions: rules,
            ..IdentityData::default()
        };
        AppData::assemble_at(1, &identity, &[], 0).unwrap()
    }

    fn grant_caller(client_id: &str) -> Caller {
        let mut caller = caller("alice", "user");
        caller.kind = CallerKind::OAuthGrant;
        caller.grant = Some(GrantContext {
            grant_id: "g1".into(),
            client_id: client_id.into(),
            scopes: OAUTH_BASELINE
                .into_iter()
                .chain([
                    Operation::CreateEmbedding,
                    Operation::CreateImage,
                    Operation::CreateSpeech,
                    Operation::ConnectRealtime,
                ])
                .map(|operation| format!("gproxy:{}", operation.id()))
                .chain(["gproxy:services".into()])
                .collect(),
            access_digest: [0; 32],
        });
        caller
    }

    #[test]
    fn oauth_consent_is_required_even_for_admins_and_known_cli_clients() {
        let mut caller = grant_caller("codex");
        caller.user_role = "admin".into();
        let cli = ["codex".into()];
        caller.grant.as_mut().unwrap().scopes.clear();
        for operation in OAUTH_BASELINE.into_iter().chain([Operation::DeleteFile]) {
            assert!(check_oauth_operation(&caller, operation, &cli).is_err());
        }
        assert!(allow_service(&snapshot(vec![]), &caller, "openai", &cli).is_err());
        caller.grant.as_mut().unwrap().scopes = vec!["gproxy:list_models".into()];
        assert!(check_oauth_operation(&caller, Operation::ListModels, &cli).is_ok());
        assert!(check_oauth_operation(&caller, Operation::GenerateContent, &cli).is_err());
        assert!(check_oauth_operation(&caller, Operation::DeleteFile, &cli).is_err());
    }

    #[test]
    fn a_provider_the_rules_do_not_reach_is_not_in_the_set() {
        let data = snapshot(vec![rule("r", "allow", Some("openai"), "*")]);
        let allowed = allowed_providers(
            &data,
            &caller("alice", "user"),
            Some("gpt-4o"),
            Operation::GenerateContent,
            &set(&["openai", "anthropic"]),
            &[],
        )
        .unwrap();
        assert_eq!(allowed, set(&["openai"]));
    }

    #[test]
    fn nothing_permitted_is_a_refusal_rather_than_an_empty_set() {
        let data = snapshot(vec![]);
        let error = allowed_providers(
            &data,
            &caller("alice", "user"),
            None,
            Operation::ListModels,
            &set(&["openai"]),
            &[],
        )
        .unwrap_err();
        assert_eq!(error.status_code(), 403);
        assert!(error.to_string().contains("no provider is permitted"));
    }

    #[test]
    fn an_instance_admin_gets_every_provider_the_instance_has() {
        // A deny that would refuse any other caller.
        let data = snapshot(vec![rule("deny", "deny", None, "*")]);
        let allowed = allowed_providers(
            &data,
            &caller("alice", "admin"),
            Some("gpt-4o"),
            Operation::GenerateContent,
            &set(&["openai", "anthropic"]),
            &[],
        )
        .unwrap();
        assert_eq!(allowed, set(&["openai", "anthropic"]));
    }

    #[test]
    fn an_oauth_client_is_held_to_the_coding_agent_baseline() {
        let data = snapshot(vec![rule("r", "allow", None, "*")]);
        let caller = grant_caller("third-party");
        for operation in OAUTH_BASELINE {
            assert!(
                allowed_providers(
                    &data,
                    &caller,
                    Some("gpt-4o"),
                    operation,
                    &set(&["openai"]),
                    &[]
                )
                .is_ok(),
                "{operation:?}"
            );
        }
        for operation in [
            Operation::CreateEmbedding,
            Operation::CreateImage,
            Operation::CreateSpeech,
            Operation::ConnectRealtime,
        ] {
            let error = allowed_providers(
                &data,
                &caller,
                Some("gpt-4o"),
                operation,
                &set(&["openai"]),
                &[],
            )
            .unwrap_err();
            assert_eq!(error.status_code(), 403, "{operation:?}");
            assert!(error.to_string().contains("not available to OAuth clients"));
        }
    }

    #[test]
    fn a_configured_cli_client_is_not_held_to_it() {
        let data = snapshot(vec![rule("r", "allow", None, "*")]);
        let cli = ["codex".to_string()];
        assert!(
            allowed_providers(
                &data,
                &grant_caller("codex"),
                Some("gpt-4o"),
                Operation::CreateEmbedding,
                &set(&["openai"]),
                &cli,
            )
            .is_ok()
        );
        // Only the named client; its neighbours are still held to it.
        assert!(
            allowed_providers(
                &data,
                &grant_caller("other"),
                Some("gpt-4o"),
                Operation::CreateEmbedding,
                &set(&["openai"]),
                &cli,
            )
            .is_err()
        );
    }

    #[test]
    fn the_baseline_holds_even_for_an_administrators_token() {
        let data = snapshot(vec![]);
        let mut caller = grant_caller("third-party");
        caller.user_role = "admin".into();
        let error = allowed_providers(
            &data,
            &caller,
            Some("gpt-4o"),
            Operation::CreateImage,
            &set(&["openai"]),
            &[],
        )
        .unwrap_err();
        assert!(error.to_string().contains("not available to OAuth clients"));
    }

    #[test]
    fn an_api_key_caller_is_not_touched_by_the_baseline() {
        let caller = caller("alice", "user");
        assert!(check_oauth_operation(&caller, Operation::CreateImage, &[]).is_ok());
    }

    #[test]
    fn a_service_needs_a_provider_wide_permission() {
        let data = snapshot(vec![rule("r", "allow", Some("openai"), "*")]);
        let alice = caller("alice", "user");
        assert!(allow_service(&data, &alice, "openai", &[]).is_ok());
        let error = allow_service(&data, &alice, "anthropic", &[]).unwrap_err();
        assert_eq!(error.status_code(), 403);

        // A model-scoped grant is about model traffic, not the account.
        let data = snapshot(vec![rule("r", "allow", Some("openai"), "gpt-*")]);
        assert!(allow_service(&data, &alice, "openai", &[]).is_err());

        // The instance administrator keeps the rule bypass.
        let data = snapshot(vec![rule("deny", "deny", None, "*")]);
        assert!(allow_service(&data, &caller("root", "admin"), "openai", &[]).is_ok());
    }

    #[test]
    fn a_service_is_outside_the_oauth_baseline() {
        let data = snapshot(vec![rule("r", "allow", None, "*")]);
        let error = allow_service(&data, &grant_caller("third-party"), "openai", &[]).unwrap_err();
        assert_eq!(error.status_code(), 403);
        assert!(error.to_string().contains("not available to OAuth clients"));

        // Not even for an administrator's token.
        let mut admin = grant_caller("third-party");
        admin.user_role = "admin".into();
        assert!(allow_service(&data, &admin, "openai", &[]).is_err());

        // A configured CLI client is not held to it, but its rules still are.
        let cli = ["codex".to_string()];
        assert!(allow_service(&data, &grant_caller("codex"), "openai", &cli).is_ok());
        assert!(allow_service(&snapshot(vec![]), &grant_caller("codex"), "openai", &cli).is_err());
    }
}
