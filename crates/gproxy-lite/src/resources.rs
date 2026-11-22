//! What a conversion may read besides the request: a file the client named by
//! id, read through the provider's own files API, or by URL. Nothing is
//! published: the engine has no storage to serve a link from.

use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, PublicationKind, PublicationStatus, PublishedResource, ResourceAccess,
        ResourceMetadata, ResourceRead, ResourceReference, Upstream,
    },
};
use http::{HeaderMap, header};

use crate::{exec::Bound, store::LIMITS};

pub(crate) struct Resources {
    pub bound: Bound,
}

fn error(kind: CapabilityErrorKind, message: impl Into<String>) -> CapabilityError {
    CapabilityError::new(kind, CapabilityErrorStage::Start, message)
}

fn metadata(headers: &HeaderMap) -> ResourceMetadata {
    let text = |name: header::HeaderName| headers.get(name)?.to_str().ok();
    ResourceMetadata {
        mime: text(header::CONTENT_TYPE)
            .and_then(|value| value.split(';').next())
            .map(|mime| mime.trim().to_owned()),
        length: text(header::CONTENT_LENGTH).and_then(|value| value.parse().ok()),
        filename: gproxy_convert::form::field(headers, header::CONTENT_DISPOSITION, "filename"),
        expires_at: None,
    }
}

/// A URL the engine is willing to fetch for a client: `http` or `https`, and
/// not an address that is plainly inside the deployment's own network. Names
/// are not resolved here, so this is no defence against a name that resolves
/// to such an address.
fn fetchable(url: &str) -> Result<http::Uri, CapabilityError> {
    let refused = || error(CapabilityErrorKind::Invalid, "this URL is not fetched");
    let uri: http::Uri = url.parse().map_err(|_| refused())?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err(refused());
    }
    let host = uri.host().ok_or_else(refused)?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let private = match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        Ok(std::net::IpAddr::V6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
        Err(_) => {
            host == "localhost" || host.ends_with(".localhost") || host.ends_with(".internal")
        }
    };
    if private {
        return Err(refused());
    }
    Ok(uri)
}

impl Resources {
    async fn file(&self, id: &str) -> Result<ResourceRead, CapabilityError> {
        let provider = &self.bound.inner.providers[self.bound.provider];
        let dialect = provider
            .channel
            .native_dialects(
                self.bound.inner.provider_view(self.bound.provider),
                Operation::RetrieveFileContent,
            )
            .into_iter()
            .find(|dialect| matches!(dialect, Dialect::OpenAi | Dialect::Claude))
            .ok_or_else(|| {
                error(
                    CapabilityErrorKind::Unsupported,
                    "the provider has no file download",
                )
            })?;
        let endpoint = gproxy_convert::file_content_endpoint(dialect, id)
            .map_err(|e| error(CapabilityErrorKind::Invalid, e.to_string()))?;
        let key = OperationKey {
            operation: Operation::RetrieveFileContent,
            dialect,
        };
        let response = self
            .bound
            .send(
                &key,
                WireRequest {
                    method: http::Method::GET,
                    path: endpoint.path,
                    query: endpoint.query,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Default::default()),
                },
            )
            .await?;
        if !response.status.is_success() {
            return Err(error(
                CapabilityErrorKind::NotFound,
                format!("the provider answered {} for file `{id}`", response.status),
            ));
        }
        Ok(ResourceRead {
            metadata: metadata(&response.headers),
            body: response.body,
        })
    }

    async fn url(&self, url: &str) -> Result<ResourceRead, CapabilityError> {
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(fetchable(url)?)
            .body(HttpBody::Bytes(Default::default()))
            .map_err(|e| error(CapabilityErrorKind::Invalid, e.to_string()))?;
        let response = self.bound.inner.client.send(request).await?;
        if !response.status.is_success() {
            return Err(error(
                CapabilityErrorKind::NotFound,
                format!("fetching the URL answered {}", response.status),
            ));
        }
        Ok(ResourceRead {
            metadata: metadata(&response.headers),
            body: response.body,
        })
    }
}

impl ResourceAccess for Resources {
    type Scope = ();
    type PublishedHandle = ();

    fn resolve<'a>(
        &'a self,
        scope: &'a (),
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        Box::pin(async move { Ok(self.read(scope, reference).await?.metadata) })
    }

    fn read<'a>(
        &'a self,
        _scope: &'a (),
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            match reference {
                ResourceReference::Id(id) => self.file(id).await,
                ResourceReference::Url(url) => self.url(url).await,
            }
        })
    }

    fn publish<'a>(
        &'a self,
        _scope: &'a (),
        _operation_id: &'a str,
        _kind: PublicationKind,
        _metadata: ResourceMetadata,
        _body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        Box::pin(async {
            Err(error(
                CapabilityErrorKind::Unsupported,
                "the lite engine stores no files, so it publishes none",
            ))
        })
    }

    fn publication_status<'a>(
        &'a self,
        _scope: &'a (),
        _operation_id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        Box::pin(async { Ok(PublicationStatus::Missing) })
    }

    fn release<'a>(
        &'a self,
        _scope: &'a (),
        _handle: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        Box::pin(async { Ok(()) })
    }

    fn limits(&self) -> CapabilityLimits {
        LIMITS
    }
}
