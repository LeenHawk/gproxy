use gproxy_channel_api::CallerIdentity;
use gproxy_core::{CacheBackend, CoreError, Target};
use gproxy_protocol::SettleMode;

use super::super::AppHost;

pub(in crate::host) async fn admit(
    host: &AppHost,
    request_id: &str,
    target: &Target,
    body: &bytes::Bytes,
    settle: SettleMode,
) -> Result<(), CoreError> {
    if settle != SettleMode::Free
        && let Some(mut state) = super::finish::load(host, request_id).await?
    {
        let identity = CallerIdentity {
            oauth_access_digest: None,
            user_id: state.identity.user_id,
            user_key_id: state.identity.user_key_id,
            org_id: state.identity.org_id,
            team_id: state.identity.team_id,
        };
        let operation = state
            .operation
            .as_deref()
            .and_then(gproxy_protocol::Operation::from_id)
            .map(|operation| {
                gproxy_protocol::OperationKey::content(
                    operation,
                    gproxy_protocol::ContentGenerationKind::ClaudeMessages,
                )
            });
        super::auth::authorize(
            &host.services.control.current(),
            &identity,
            operation,
            &gproxy_core::Plan {
                targets: vec![target.clone()],
                budget: gproxy_core::control::FailoverBudget { max_attempts: 1 },
            },
        )?;
        let mut charged = Vec::new();
        let extra =
            match super::quota::reserve_retry(host, &identity, body, target, &mut charged).await {
                Ok(extra) => extra,
                Err(error) => return super::reserve::rollback_error(host, charged, error).await,
            };
        let expected = serde_json::to_vec(&state).expect("admission state serializes");
        state.reservations.extend(extra);
        let updated = serde_json::to_vec(&state).expect("admission state serializes");
        let result = host
            .services
            .cache
            .compare_and_swap(
                &super::types::reservation_key(request_id),
                Some(expected),
                Some(updated),
                None,
            )
            .await;
        match result {
            Ok(true) => {}
            Ok(false) => {
                return super::reserve::rollback_error(
                    host,
                    charged,
                    CoreError::Internal("admission changed during fallback".into()),
                )
                .await;
            }
            Err(error) => return super::reserve::rollback_error(host, charged, error.into()).await,
        }
    }
    super::credential::admit(host, request_id, target, body, settle).await
}
