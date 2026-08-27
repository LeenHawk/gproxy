use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_core::UpstreamTransport;
use gproxy_tokenize::{
    BoxFuture, RegistryError, TokenizerClient, TokenizerRegistry, TokenizerStore,
};

struct StoreAdapter(gproxy_store::Store);

impl TokenizerStore for StoreAdapter {
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>, RegistryError>> {
        Box::pin(async move { self.0.tokenizer_vocab_names().await.map_err(registry_error) })
    }

    fn get<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, RegistryError>> {
        Box::pin(async move { self.0.tokenizer_vocab(name).await.map_err(registry_error) })
    }

    fn put<'a>(
        &'a self,
        name: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), RegistryError>> {
        Box::pin(async move {
            self.0
                .put_tokenizer_vocab(name, bytes)
                .await
                .map_err(registry_error)
        })
    }

    fn quarantine<'a>(
        &'a self,
        name: &'a str,
        reason: &'a str,
    ) -> BoxFuture<'a, Result<(), RegistryError>> {
        Box::pin(async move {
            tracing::warn!(name, reason, "quarantining invalid tokenizer vocabulary");
            self.0
                .delete_tokenizer_vocab(name)
                .await
                .map_err(registry_error)
        })
    }
}

struct ClientAdapter(gproxy_upstream::Transport);

impl TokenizerClient for ClientAdapter {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
        Box::pin(async move {
            let response = self.0.send(request).await.map_err(registry_error)?;
            let (parts, mut stream) = response.into_parts();
            let mut body = BytesMut::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(registry_error)?;
                if body.len().saturating_add(chunk.len()) > gproxy_tokenize::MAX_TOKENIZER_BYTES {
                    return Err(RegistryError::new("tokenizer download exceeds 16 MiB"));
                }
                body.extend_from_slice(&chunk);
            }
            Ok(http::Response::from_parts(parts, body.freeze()))
        })
    }
}

pub(crate) fn build(
    store: gproxy_store::Store,
    transport: gproxy_upstream::Transport,
    download_enabled: bool,
) -> Arc<TokenizerRegistry> {
    let registry = Arc::new(TokenizerRegistry::new(
        Arc::new(StoreAdapter(store)),
        Arc::new(ClientAdapter(transport)),
    ));
    registry.set_download_enabled(download_enabled);
    registry.preheat();
    registry
}

fn registry_error(error: impl std::fmt::Display) -> RegistryError {
    RegistryError::new(error.to_string())
}
