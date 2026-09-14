use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use gproxy_protocol::{HttpBody, capability::*};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
pub const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
pub fn png() -> Bytes {
    STANDARD.decode(PNG).unwrap().into()
}
#[derive(Default)]
pub struct Resources {
    pub reads: AtomicUsize,
    pub publishes: AtomicUsize,
    pub statuses: AtomicUsize,
    pub hang_publish: AtomicBool,
    pub hung: AtomicBool,
    pub omit_read_mime: AtomicBool,
    pub read_feed: Mutex<Option<super::http_host::Feed>>,
    pub read_expiry: Mutex<Option<std::time::SystemTime>>,
    entries: Mutex<BTreeMap<(u8, String), PublishedResource<String>>>,
}
impl Resources {
    pub fn contradict_publication_metadata(&self) {
        for value in self.entries.lock().unwrap().values_mut() {
            value.metadata.length = Some(0);
        }
    }
}
impl ResourceAccess for Resources {
    type Scope = u8;
    type PublishedHandle = String;
    fn resolve<'a>(
        &'a self,
        _: &'a u8,
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        panic!("unexpected resolve")
    }
    fn read<'a>(
        &'a self,
        scope: &'a u8,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(*scope, 1);
            assert_eq!(
                reference,
                &ResourceReference::Url("gs://private/generated.png".into())
            );
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: if self.omit_read_mime.load(Ordering::SeqCst) {
                        None
                    } else {
                        Some("image/png".into())
                    },
                    length: Some(png().len() as u64),
                    filename: None,
                    expires_at: *self.read_expiry.lock().unwrap(),
                },
                body: self
                    .read_feed
                    .lock()
                    .unwrap()
                    .take()
                    .map(|feed| HttpBody::Stream(Box::pin(feed)))
                    .unwrap_or_else(|| HttpBody::Bytes(png())),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        scope: &'a u8,
        operation: &'a str,
        kind: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<String>, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(kind, PublicationKind::Url);
            let HttpBody::Bytes(bytes) = body else {
                panic!("unexpected streamed publication")
            };
            assert_eq!(bytes, png());
            self.publishes.fetch_add(1, Ordering::SeqCst);
            let value = PublishedResource {
                handle: operation.into(),
                reference: ResourceReference::Url(format!("https://published.example/{operation}")),
                metadata,
            };
            assert!(
                self.entries
                    .lock()
                    .unwrap()
                    .insert((*scope, operation.into()), value.clone())
                    .is_none(),
                "adapter replayed publication instead of status recovery"
            );
            if self.hang_publish.swap(false, Ordering::SeqCst) {
                self.hung.store(true, Ordering::SeqCst);
                return std::future::pending().await;
            }
            Ok(value)
        })
    }
    fn publication_status<'a>(
        &'a self,
        scope: &'a u8,
        operation: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<String>, CapabilityError>> {
        Box::pin(async move {
            self.statuses.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(&(*scope, operation.into()))
                .cloned()
                .map(PublicationStatus::Published)
                .unwrap_or(PublicationStatus::Missing))
        })
    }
    fn release<'a>(
        &'a self,
        _: &'a u8,
        _: &'a String,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!("adapter must not release published handles implicitly")
    }
    fn limits(&self) -> CapabilityLimits {
        super::http_host::limits()
    }
}
