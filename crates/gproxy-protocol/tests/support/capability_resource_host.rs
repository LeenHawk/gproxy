use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use bytes::Bytes;
use gproxy_protocol::{
    HttpBody,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, PublicationKind, PublicationStatus, PublishedResource, ResourceAccess,
        ResourceMetadata, ResourceRead, ResourceReference,
    },
};

type Key = (String, String);

#[derive(Clone)]
enum Stored {
    Pending(PublishedResource<String>),
    Published(PublishedResource<String>),
    Expired(PublishedResource<String>),
}

#[derive(Clone)]
pub struct ResourceFake {
    state: Arc<Mutex<HashMap<Key, Stored>>>,
    pending_once: Arc<AtomicBool>,
    cancelled_once: Arc<AtomicBool>,
    body_consumed: Arc<AtomicUsize>,
    support_url: bool,
}

impl ResourceFake {
    pub fn new(support_url: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            pending_once: Arc::new(AtomicBool::new(true)),
            cancelled_once: Arc::new(AtomicBool::new(true)),
            body_consumed: Arc::new(AtomicUsize::new(0)),
            support_url,
        }
    }

    pub fn metadata() -> ResourceMetadata {
        ResourceMetadata {
            mime: Some("text/plain".into()),
            length: Some(3),
            filename: Some("answer.txt".into()),
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
        }
    }

    pub fn body_count(&self) -> usize {
        self.body_consumed.load(Ordering::SeqCst)
    }

    pub fn expire(&self, scope: &str, operation_id: &str) {
        let key = (scope.to_owned(), operation_id.to_owned());
        let mut state = self.state.lock().unwrap();
        if let Some(stored) = state.get_mut(&key) {
            let published = match stored {
                Stored::Pending(published) | Stored::Published(published) => published,
                Stored::Expired(_) => return,
            };
            published.metadata.expires_at = Some(SystemTime::now() - Duration::from_secs(1));
        }
    }

    fn error(kind: CapabilityErrorKind, message: &'static str) -> CapabilityError {
        CapabilityError::new(kind, CapabilityErrorStage::Start, message)
    }

    fn kind_matches(reference: &ResourceReference, kind: PublicationKind) -> bool {
        matches!(
            (reference, kind),
            (ResourceReference::Id(_), PublicationKind::Id)
                | (ResourceReference::Url(_), PublicationKind::Url)
        )
    }

    fn expired(metadata: &ResourceMetadata) -> bool {
        metadata
            .expires_at
            .is_some_and(|at| at <= SystemTime::now())
    }

    fn consume(body: HttpBody, counter: &AtomicUsize) {
        if let HttpBody::Bytes(bytes) = body {
            counter.fetch_add(bytes.len(), Ordering::SeqCst);
        }
    }
}

impl ResourceAccess for ResourceFake {
    type Scope = String;
    type PublishedHandle = String;

    fn resolve<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        let mut state = self.state.lock().unwrap();
        let key = state.iter().find_map(|(key, stored)| {
            if key.0 != *scope {
                return None;
            }
            match stored {
                Stored::Published(published) if &published.reference == reference => {
                    Some((key.clone(), stored.clone()))
                }
                Stored::Expired(published) if &published.reference == reference => {
                    Some((key.clone(), stored.clone()))
                }
                _ => None,
            }
        });
        let result = match key {
            Some((key, Stored::Published(published))) if &published.reference == reference => {
                if Self::expired(&published.metadata) {
                    state.insert(key, Stored::Expired(published.clone()));
                    Err(Self::error(
                        CapabilityErrorKind::Expired,
                        "resource expired",
                    ))
                } else {
                    Ok(published.metadata)
                }
            }
            Some((_, Stored::Expired(_))) => Err(Self::error(
                CapabilityErrorKind::Expired,
                "resource expired",
            )),
            _ => Err(Self::error(CapabilityErrorKind::NotFound, "missing")),
        };
        Box::pin(async move { result })
    }

    fn read<'a>(
        &'a self,
        scope: &'a Self::Scope,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        let metadata = self.resolve(scope, reference);
        Box::pin(async move {
            Ok(ResourceRead {
                metadata: metadata.await?,
                body: HttpBody::Bytes(Bytes::from_static(b"abc")),
            })
        })
    }

    fn publish<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
        kind: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<Self::PublishedHandle>, CapabilityError>>
    {
        let key = (scope.clone(), operation_id.to_owned());
        let mut state = self.state.lock().unwrap();
        if let Some(existing) = state.get(&key).cloned() {
            let result = match existing {
                Stored::Pending(published) if Self::expired(&published.metadata) => {
                    state.insert(key, Stored::Expired(published));
                    Err(Self::error(
                        CapabilityErrorKind::Expired,
                        "publication expired",
                    ))
                }
                Stored::Pending(_) => Err(Self::error(
                    CapabilityErrorKind::Conflict,
                    "publication pending",
                )),
                Stored::Expired(_) => Err(Self::error(
                    CapabilityErrorKind::Expired,
                    "publication expired",
                )),
                Stored::Published(published) if Self::expired(&published.metadata) => {
                    state.insert(key, Stored::Expired(published.clone()));
                    Err(Self::error(
                        CapabilityErrorKind::Expired,
                        "publication expired",
                    ))
                }
                Stored::Published(published) if Self::kind_matches(&published.reference, kind) => {
                    Ok(published)
                }
                Stored::Published(_) => Err(Self::error(
                    CapabilityErrorKind::Conflict,
                    "operation id has another reference kind",
                )),
            };
            return Box::pin(async move { result });
        }
        if metadata.expires_at.is_none_or(|at| at <= SystemTime::now()) {
            return Box::pin(async {
                Err(Self::error(CapabilityErrorKind::Invalid, "expiry required"))
            });
        }
        let reference = match kind {
            PublicationKind::Id => ResourceReference::Id(format!("ref:{operation_id}")),
            PublicationKind::Url if self.support_url => {
                ResourceReference::Url(format!("https://files/{operation_id}"))
            }
            PublicationKind::Url => {
                return Box::pin(async {
                    Err(Self::error(
                        CapabilityErrorKind::Unsupported,
                        "URL unsupported",
                    ))
                });
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Box::pin(async {
                    Err(Self::error(
                        CapabilityErrorKind::Unsupported,
                        "unknown kind",
                    ))
                });
            }
        };
        Self::consume(body, &self.body_consumed);
        let published = PublishedResource {
            handle: format!("{scope}:{operation_id}"),
            reference,
            metadata,
        };
        let pending = operation_id == "pending" && self.pending_once.swap(false, Ordering::SeqCst);
        let cancelled =
            operation_id == "cancelled" && self.cancelled_once.swap(false, Ordering::SeqCst);
        state.insert(
            key,
            if pending {
                Stored::Pending(published.clone())
            } else {
                Stored::Published(published.clone())
            },
        );
        if pending {
            Box::pin(async { std::future::pending().await })
        } else if cancelled {
            Box::pin(async { std::future::pending().await })
        } else {
            Box::pin(async move { Ok(published) })
        }
    }

    fn publication_status<'a>(
        &'a self,
        scope: &'a Self::Scope,
        operation_id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<Self::PublishedHandle>, CapabilityError>>
    {
        let key = (scope.clone(), operation_id.to_owned());
        let mut state = self.state.lock().unwrap();
        let result = match state.get(&key).cloned() {
            Some(Stored::Pending(published)) if Self::expired(&published.metadata) => {
                state.insert(key, Stored::Expired(published));
                PublicationStatus::Expired
            }
            Some(Stored::Pending(_)) => PublicationStatus::Pending,
            Some(Stored::Expired(_)) => PublicationStatus::Expired,
            Some(Stored::Published(published)) if Self::expired(&published.metadata) => {
                state.insert(key, Stored::Expired(published.clone()));
                PublicationStatus::Expired
            }
            Some(Stored::Published(published)) => PublicationStatus::Published(published),
            None => PublicationStatus::Missing,
        };
        Box::pin(async move { Ok(result) })
    }

    fn release<'a>(
        &'a self,
        scope: &'a Self::Scope,
        handle: &'a Self::PublishedHandle,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        let mut state = self.state.lock().unwrap();
        let key = state.iter().find_map(|(key, stored)| match stored {
            Stored::Published(published) if key.0 == *scope && published.handle == *handle => {
                Some(key.clone())
            }
            Stored::Expired(published) if key.0 == *scope && published.handle == *handle => {
                Some(key.clone())
            }
            _ => None,
        });
        let result = match key {
            Some(key) => match state.get(&key).cloned() {
                Some(Stored::Published(published)) if Self::expired(&published.metadata) => {
                    state.insert(key, Stored::Expired(published.clone()));
                    Err(Self::error(
                        CapabilityErrorKind::Expired,
                        "publication expired",
                    ))
                }
                Some(Stored::Published(published)) => {
                    state.insert(key, Stored::Expired(published));
                    Ok(())
                }
                Some(Stored::Expired(_)) => Err(Self::error(
                    CapabilityErrorKind::Expired,
                    "publication expired",
                )),
                _ => Err(Self::error(CapabilityErrorKind::Invalid, "invalid handle")),
            },
            None => Err(Self::error(CapabilityErrorKind::Invalid, "invalid handle")),
        };
        Box::pin(async move { result })
    }

    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(5),
            stream_idle: Duration::from_secs(1),
            read_bytes: 1024,
            write_bytes: 1024,
            ws_frame_bytes: 1024,
        }
    }
}
