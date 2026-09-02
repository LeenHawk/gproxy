use gproxy_admin::AdminError;
use gproxy_channel_api::BoxFuture;

use crate::AppHandle;

pub(super) fn configured(app: &AppHandle) -> BoxFuture<'_, Result<bool, AdminError>> {
    Box::pin(async move {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = app;
            return Err(AdminError::Forbidden);
        }
        #[cfg(not(target_arch = "wasm32"))]
        Ok(app
            .inner
            .host
            .services
            .store
            .tokenizer_auth(crate::host::tokenizers::HUGGING_FACE_AUTH)
            .await?
            .is_some())
    })
}

pub(super) fn update<'a>(
    app: &'a AppHandle,
    token: Option<&'a str>,
) -> BoxFuture<'a, Result<bool, AdminError>> {
    Box::pin(async move {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (app, token);
            return Err(AdminError::Forbidden);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let store = &app.inner.host.services.store;
            match token {
                Some(token) => {
                    let envelope = app
                        .inner
                        .host
                        .services
                        .cipher
                        .seal(&serde_json::Value::String(token.to_owned()))
                        .map_err(|error| AdminError::Internal(error.to_string()))?;
                    store
                        .put_tokenizer_auth(crate::host::tokenizers::HUGGING_FACE_AUTH, &envelope)
                        .await?;
                }
                None => {
                    store
                        .delete_tokenizer_auth(crate::host::tokenizers::HUGGING_FACE_AUTH)
                        .await?;
                }
            }
            app.inner
                .host
                .services
                .tokenizers
                .set_hugging_face_token(token.map(str::to_owned));
            Ok(token.is_some())
        }
    })
}

pub(super) fn reveal(app: &AppHandle) -> BoxFuture<'_, Result<String, AdminError>> {
    Box::pin(async move {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = app;
            return Err(AdminError::Forbidden);
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::host::tokenizers::hugging_face_token(
            &app.inner.host.services.store,
            &app.inner.host.services.cipher,
        )
        .await
        .map_err(|error| AdminError::Internal(error.to_string()))?
        .ok_or(AdminError::NotFound)
    })
}
