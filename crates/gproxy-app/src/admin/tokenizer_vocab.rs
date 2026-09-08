use gproxy_admin::{AdminError, dto::TokenizerVocabDto};

pub(super) async fn fetch(
    app: &crate::AppHandle,
    name: &str,
    repository: &str,
) -> Result<TokenizerVocabDto, AdminError> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (app, name, repository);
        Err(AdminError::Forbidden)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if name.is_empty() {
            return Err(AdminError::BadRequest(
                "tokenizer vocabulary name must not be blank".into(),
            ));
        }
        if repository.is_empty() {
            return Err(AdminError::BadRequest(
                "tokenizer repository must not be blank".into(),
            ));
        }
        let registry = &app.inner.host.services.tokenizers;
        registry.fetch(name, repository).await.map_err(|error| {
            tracing::warn!(name, repository, %error, "manual tokenizer fetch failed");
            AdminError::BadRequest("tokenizer vocabulary could not be fetched".into())
        })?;
        app.inner
            .host
            .services
            .store
            .tokenizer_vocabs()
            .await?
            .into_iter()
            .find(|vocab| vocab.name == name)
            .map(super::helpers::tokenizer_dto)
            .ok_or(AdminError::NotFound)
    }
}
