use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, SystemTime},
};

use bytes::Bytes;
use futures_core::Stream;
use gproxy_protocol::{
    HttpBody,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, PublicationKind,
        PublicationStatus, ResourceAccess,
    },
    connection::TransportError,
};

#[path = "support/capability_resource_host.rs"]
mod host;
use host::ResourceFake;

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("synchronous contract fake unexpectedly pending"),
    }
}

struct NeverPoll;

impl Stream for NeverPoll {
    type Item = Result<Bytes, TransportError>;

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        panic!("rejected or duplicate body was polled")
    }
}

fn never_polled_body() -> HttpBody {
    HttpBody::Stream(Box::pin(NeverPoll))
}

#[derive(Debug)]
struct SourceError;

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("source identity")
    }
}

impl Error for SourceError {}

#[test]
fn capability_error_preserves_an_existing_transport_source() {
    let source: gproxy_protocol::connection::TransportError = Box::new(SourceError);
    let error = CapabilityError::with_source(
        CapabilityErrorKind::Transport,
        CapabilityErrorStage::BodyTransfer,
        "body failed",
        source,
    );
    assert!(
        error
            .source_error()
            .unwrap()
            .downcast_ref::<SourceError>()
            .is_some()
    );
}

#[test]
fn cancellation_status_duplicate_metadata_and_release_tombstone_are_stable() {
    let fake = ResourceFake::new(true);
    let scope = "session-a".to_owned();
    let mut original = ResourceFake::metadata();
    original.filename = Some("original.txt".into());
    let operation = fake.publish(
        &scope,
        "cancelled",
        PublicationKind::Id,
        original.clone(),
        HttpBody::Bytes(Bytes::from_static(b"abc")),
    );
    drop(operation);
    let PublicationStatus::Published(first) =
        ready(fake.publication_status(&scope, "cancelled")).unwrap()
    else {
        panic!("publication was lost")
    };
    let mut replacement = ResourceFake::metadata();
    replacement.filename = Some("replacement.txt".into());
    let duplicate = ready(fake.publish(
        &scope,
        "cancelled",
        PublicationKind::Id,
        replacement,
        HttpBody::Bytes(Bytes::from_static(b"replay")),
    ))
    .unwrap();
    assert_eq!(duplicate, first);
    assert_eq!(fake.body_count(), 3);
    assert_eq!(
        ready(fake.publish(
            &scope,
            "cancelled",
            PublicationKind::Url,
            ResourceFake::metadata(),
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Conflict
    );
    assert!(ready(fake.release(&scope, &first.handle)).is_ok());
    assert_eq!(
        ready(fake.publication_status(&scope, "cancelled")).unwrap(),
        PublicationStatus::Expired
    );
    assert_eq!(
        ready(fake.publish(
            &scope,
            "cancelled",
            PublicationKind::Id,
            ResourceFake::metadata(),
            HttpBody::Bytes(Bytes::from_static(b"again")),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Expired
    );
}

#[test]
fn pending_publication_is_recoverable_and_duplicate_does_not_consume_body() {
    let fake = ResourceFake::new(true);
    let scope = "session-a".to_owned();
    drop(fake.publish(
        &scope,
        "pending",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"pending")),
    ));
    assert_eq!(
        ready(fake.publication_status(&scope, "pending")).unwrap(),
        PublicationStatus::Pending
    );
    assert_eq!(
        ready(fake.publish(
            &scope,
            "pending",
            PublicationKind::Id,
            ResourceFake::metadata(),
            HttpBody::Bytes(Bytes::from_static(b"duplicate")),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Conflict
    );
    assert_eq!(fake.body_count(), 7);
    fake.expire(&scope, "pending");
    assert_eq!(
        ready(fake.publication_status(&scope, "pending")).unwrap(),
        PublicationStatus::Expired
    );
    assert_eq!(
        ready(fake.publish(
            &scope,
            "pending",
            PublicationKind::Id,
            ResourceFake::metadata(),
            HttpBody::Bytes(Bytes::from_static(b"after-expiry")),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Expired
    );
}

#[test]
fn preflight_rejects_without_consuming_and_existing_state_wins_new_metadata() {
    let fake = ResourceFake::new(false);
    let scope = "session-a".to_owned();
    let body_before = fake.body_count();
    assert_eq!(
        ready(fake.publish(
            &scope,
            "url",
            PublicationKind::Url,
            ResourceFake::metadata(),
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Unsupported
    );
    let mut none = ResourceFake::metadata();
    none.expires_at = None;
    assert_eq!(
        ready(fake.publish(
            &scope,
            "new-none",
            PublicationKind::Id,
            none,
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Invalid
    );
    let mut past = ResourceFake::metadata();
    past.expires_at = Some(SystemTime::now() - Duration::from_secs(1));
    assert_eq!(
        ready(fake.publish(
            &scope,
            "new-past",
            PublicationKind::Id,
            past,
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Invalid
    );
    assert_eq!(fake.body_count(), body_before);

    let first = ready(fake.publish(
        &scope,
        "existing",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"first")),
    ))
    .unwrap();
    let mut invalid_duplicate = ResourceFake::metadata();
    invalid_duplicate.expires_at = None;
    let same = ready(fake.publish(
        &scope,
        "existing",
        PublicationKind::Id,
        invalid_duplicate,
        never_polled_body(),
    ))
    .unwrap();
    assert_eq!(same, first);
    assert_eq!(
        ready(fake.publish(
            &scope,
            "existing",
            PublicationKind::Url,
            ResourceFake::metadata(),
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Conflict
    );
    assert_eq!(fake.body_count(), 5);
}

#[test]
fn scope_is_exact_and_expiry_ends_in_a_tombstone() {
    let fake = ResourceFake::new(false);
    let owner = "session-a:child".to_owned();
    let colliding = "session-a".to_owned();
    let published = ready(fake.publish(
        &owner,
        "child",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"abc")),
    ))
    .unwrap();
    assert!(ready(fake.resolve(&owner, &published.reference)).is_ok());
    let second = ready(fake.publish(
        &owner,
        "other",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"other")),
    ))
    .unwrap();
    assert!(ready(fake.resolve(&owner, &second.reference)).is_ok());
    assert_eq!(
        ready(fake.resolve(&colliding, &published.reference))
            .unwrap_err()
            .kind(),
        CapabilityErrorKind::NotFound
    );
    assert_eq!(
        ready(fake.read(&colliding, &published.reference))
            .unwrap_err()
            .kind(),
        CapabilityErrorKind::NotFound
    );

    fake.expire(&owner, "child");
    assert_eq!(
        ready(fake.release(&owner, &published.handle))
            .unwrap_err()
            .kind(),
        CapabilityErrorKind::Expired
    );
    assert_eq!(
        ready(fake.publication_status(&owner, "child")).unwrap(),
        PublicationStatus::Expired
    );
    assert_eq!(
        ready(fake.publish(
            &owner,
            "child",
            PublicationKind::Id,
            ResourceFake::metadata(),
            HttpBody::Bytes(Bytes::from_static(b"recreate")),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Expired
    );
}

#[test]
fn publish_checks_stored_expiry_without_an_intermediate_read() {
    let fake = ResourceFake::new(true);
    let scope = "session-a".to_owned();
    ready(fake.publish(
        &scope,
        "stored-expired",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"first")),
    ))
    .unwrap();
    fake.expire(&scope, "stored-expired");
    assert_eq!(
        ready(fake.publish(
            &scope,
            "stored-expired",
            PublicationKind::Id,
            ResourceFake::metadata(),
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Expired
    );

    drop(fake.publish(
        &scope,
        "pending",
        PublicationKind::Id,
        ResourceFake::metadata(),
        HttpBody::Bytes(Bytes::from_static(b"pending")),
    ));
    fake.expire(&scope, "pending");
    assert_eq!(
        ready(fake.publish(
            &scope,
            "pending",
            PublicationKind::Id,
            ResourceFake::metadata(),
            never_polled_body(),
        ))
        .unwrap_err()
        .kind(),
        CapabilityErrorKind::Expired
    );
}

#[test]
fn concurrent_reference_kinds_have_one_atomic_winner() {
    let fake = ResourceFake::new(true);
    let scope = "session-a".to_owned();
    let left = fake.clone();
    let right = fake.clone();
    let (a, b) = std::thread::scope(|threads| {
        let first = threads.spawn(|| {
            ready(left.publish(
                &scope,
                "race",
                PublicationKind::Id,
                ResourceFake::metadata(),
                HttpBody::Bytes(Bytes::from_static(b"id")),
            ))
        });
        let second = threads.spawn(|| {
            ready(right.publish(
                &scope,
                "race",
                PublicationKind::Url,
                ResourceFake::metadata(),
                HttpBody::Bytes(Bytes::from_static(b"url")),
            ))
        });
        (first.join().unwrap(), second.join().unwrap())
    });
    let (a_won, conflict) = match (a, b) {
        (Ok(_), Err(error)) => (true, error),
        (Err(error), Ok(_)) => (false, error),
        _ => panic!("exactly one publication kind must win"),
    };
    assert_eq!(conflict.kind(), CapabilityErrorKind::Conflict);
    assert_eq!(fake.body_count(), if a_won { 2 } else { 3 });
}
