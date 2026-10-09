//! Refresh upstream candidates before serving the configured downstream catalog.
use std::collections::BTreeSet;

use gproxy_protocol::Operation;
use gproxy_seaorm::BatchConnectionTrait;

use crate::{
    App, AppError, Caller,
    admission::{credential, permission},
};

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    /// Refresh only providers and credentials this caller may reach. A failed
    /// upstream refresh leaves the saved catalog available.
    pub async fn refresh_model_catalog(
        &self,
        caller: &Caller,
        providers: &BTreeSet<String>,
        operation: Operation,
        model: Option<&str>,
    ) -> Result<(), AppError> {
        let data = self.data();
        permission::check_oauth_operation(caller, operation, &self.config().oauth.cli_client_ids)?;
        let allowed = match permission::allowed_providers(
            &data,
            caller,
            model,
            operation,
            providers,
            &self.config().oauth.cli_client_ids,
        ) {
            Ok(allowed) => allowed,
            // An empty catalog remains a valid list; no upstream is contacted.
            Err(AppError::Forbidden(_)) => return Ok(()),
            Err(error) => return Err(error),
        };
        let core = self.gproxy().core().snapshot();
        let credentials = credential::visible_credentials(
            &data,
            caller,
            &core.credentials.keys().cloned().collect(),
        );
        for id in allowed {
            let Some(provider) = core.providers.get(&id) else {
                continue;
            };
            if provider
                .entity
                .config
                .get("auto_refresh_models")
                .and_then(serde_json::Value::as_bool)
                == Some(false)
                || !provider
                    .credential_ids
                    .iter()
                    .any(|id| credentials.contains(id))
                || provider
                    .channel
                    .local_operations()
                    .contains(&Operation::ListModels)
            {
                continue;
            }
            if let Err(error) = self
                .gproxy()
                .manage()
                .connectivity()
                .refresh_models(&id, &credentials)
                .await
            {
                tracing::warn!(provider_id = %id, %error, "model catalog refresh failed; using saved models");
            }
        }
        Ok(())
    }
}
