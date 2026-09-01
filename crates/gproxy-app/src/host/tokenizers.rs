use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_core::UpstreamTransport;
use gproxy_tokenize::{
    BoxFuture, ProgressReporter, RegistryError, TokenizerClient, TokenizerDownloadProgress,
    TokenizerRegistry, TokenizerStore,
};

struct StoreAdapter(gproxy_store::Store);

impl TokenizerStore for StoreAdapter {
    fn list<'a>(&'a self) -> BoxFuture<'a, Result<Vec<String>, RegistryError>> {
        Box::pin(async move { self.0.tokenizer_vocab_names().await.map_err(registry_error) })
    }

    fn get<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<Option<gproxy_tokenize::StoredTokenizer>, RegistryError>> {
        Box::pin(async move {
            self.0
                .tokenizer_vocab(name)
                .await
                .map(|value| {
                    value.map(|value| gproxy_tokenize::StoredTokenizer {
                        repository: value.repository,
                        bytes: value.bytes,
                    })
                })
                .map_err(registry_error)
        })
    }

    fn put<'a>(
        &'a self,
        name: &'a str,
        repository: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<(), RegistryError>> {
        Box::pin(async move {
            self.0
                .put_tokenizer_vocab(name, repository, bytes)
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
        progress: Option<ProgressReporter>,
    ) -> BoxFuture<'a, Result<http::Response<Bytes>, RegistryError>> {
        Box::pin(async move {
            let response = self.0.send(request).await.map_err(registry_error)?;
            let (parts, mut stream) = response.into_parts();
            let report_progress = parts.status.is_success();
            let total_bytes = parts
                .headers
                .get(http::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            if total_bytes.is_some_and(|size| size > gproxy_tokenize::MAX_TOKENIZER_BYTES as u64) {
                return Err(RegistryError::new("tokenizer download exceeds 32 MiB"));
            }
            let mut body = BytesMut::new();
            if report_progress && let Some(report) = &progress {
                report(TokenizerDownloadProgress {
                    downloaded_bytes: 0,
                    total_bytes,
                });
            }
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(registry_error)?;
                if body.len().saturating_add(chunk.len()) > gproxy_tokenize::MAX_TOKENIZER_BYTES {
                    return Err(RegistryError::new("tokenizer download exceeds 32 MiB"));
                }
                body.extend_from_slice(&chunk);
                if report_progress && let Some(report) = &progress {
                    report(TokenizerDownloadProgress {
                        downloaded_bytes: body.len() as u64,
                        total_bytes,
                    });
                }
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
