#![cfg(not(target_arch = "wasm32"))]

//! `Resources`: publications over Store rows and the file backend, upstream
//! reads by id through the scope's provider, and the contract's edge cases.

mod support;
use gproxy_core::{ExecutionLimits, PublishedHandle, ResourceScope, Resources};
use gproxy_protocol::{
    HttpBody,
    capability::{
        CapabilityErrorKind, PublicationKind, PublicationStatus, ResourceAccess, ResourceMetadata,
        ResourceReference,
    },
    connection::Bytes,
};
use http::StatusCode;
use serde_json::json;
use std::time::{Duration, SystemTime};
use support::*;

fn metadata() -> ResourceMetadata {
    ResourceMetadata {
        mime: Some("image/png".into()),
        length: None,
        filename: Some("out.png".into()),
        expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
    }
}

fn scope(h: &Harness, scope: &str, provider: &str) -> ResourceScope {
    ResourceScope {
        scope: scope.into(),
        target: h.target(provider),
    }
}

fn resources(h: &Harness) -> Resources<'_, sea_orm::DatabaseConnection> {
    let limits = ExecutionLimits::default();
    Resources::new(&h.core, limits.capability(None), limits.codec())
}

fn body(text: &'static str) -> HttpBody {
    HttpBody::Bytes(Bytes::from_static(text.as_bytes()))
}

async fn with_storage() -> (Harness, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let operator = gproxy_file::filesystem(dir.path().to_str().unwrap()).unwrap();
    let h = harness_with_storage(full(), "round_robin", Some(operator)).await;
    (h, dir)
}

async fn with_links(base: Option<&'static str>) -> (Harness, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let operator = gproxy_file::filesystem(dir.path().to_str().unwrap()).unwrap();
    let h = harness_with_publication(
        full(),
        "round_robin",
        Some(operator),
        Some(std::sync::Arc::new(FixedLinks(base))),
    )
    .await;
    (h, dir)
}

fn stored_objects(dir: &tempfile::TempDir) -> usize {
    walkdir(dir.path())
}

fn walkdir(path: &std::path::Path) -> usize {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    if e.path().is_dir() {
                        walkdir(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

#[tokio::test]
async fn url_publication_stores_the_body_and_reads_back_through_core() {
    let (h, dir) = with_links(Some("https://files.example")).await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");

    let published = r
        .publish(
            &s,
            "op-url",
            PublicationKind::Url,
            metadata(),
            body("png-bytes"),
        )
        .await
        .unwrap();
    let id = published.handle.binding_id.clone();
    assert_eq!(
        published.reference,
        ResourceReference::Url(format!("https://files.example/{id}"))
    );
    assert_eq!(published.metadata.length, Some(9));
    assert_eq!(published.metadata.mime.as_deref(), Some("image/png"));
    assert_eq!(stored_objects(&dir), 1);

    // The binding row records the publication and its link for replays.
    let row = h
        .core
        .store()
        .resource_bindings()
        .get_many(std::slice::from_ref(&id))
        .await
        .unwrap()[0]
        .clone()
        .unwrap();
    assert_eq!(row.kind, "publication");
    assert_eq!(row.public_id, "op-url");
    assert!(row.file_id.is_some());
    let again = r
        .publish(
            &s,
            "op-url",
            PublicationKind::Url,
            metadata(),
            body("ignored"),
        )
        .await
        .unwrap();
    assert_eq!(again, published);
    let err = r
        .publish(&s, "op-url", PublicationKind::Id, metadata(), body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Conflict);

    // The host's download route reads the bytes back by id, without a scope.
    let read = h.core.read_publication(&id).await.unwrap().unwrap();
    assert_eq!(read.id, id);
    assert_eq!(read.scope, s.column());
    assert_eq!(read.metadata, published.metadata);
    assert_eq!(support::read(read.body).await, "png-bytes");
    assert!(h.core.read_publication("nope").await.unwrap().is_none());

    // Deleting tombstones the row and removes the object.
    assert!(h.core.delete_publication(&id).await.unwrap());
    assert!(!h.core.delete_publication(&id).await.unwrap());
    assert!(h.core.read_publication(&id).await.unwrap().is_none());
    assert_eq!(stored_objects(&dir), 0);
    assert_eq!(
        r.publication_status(&s, "op-url").await.unwrap(),
        PublicationStatus::Expired
    );
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn url_publication_refused_by_the_host_leaves_nothing_behind() {
    let (h, dir) = with_links(None).await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let err = r
        .publish(&s, "op-url", PublicationKind::Url, metadata(), body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Unsupported);
    assert_eq!(stored_objects(&dir), 0);
    assert_eq!(
        r.publication_status(&s, "op-url").await.unwrap(),
        PublicationStatus::Missing
    );
    let rows = h
        .core
        .store()
        .resource_bindings()
        .query(<gproxy_store::entity::resource::resource_binding::Entity as sea_orm::EntityTrait>::find())
        .await
        .unwrap();
    assert!(rows.is_empty());
    // Id publication is unaffected by the host's refusal of URLs.
    r.publish(&s, "op-id", PublicationKind::Id, metadata(), body("x"))
        .await
        .unwrap();
}

#[tokio::test]
async fn expired_publication_reads_as_none_through_core() {
    let (h, _dir) = with_links(Some("https://files.example")).await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let published = r
        .publish(&s, "op-url", PublicationKind::Url, metadata(), body("x"))
        .await
        .unwrap();
    let id = published.handle.binding_id.clone();
    assert!(h.core.read_publication(&id).await.unwrap().is_some());
    h.core
        .store()
        .resource_bindings()
        .update_many(vec![
            gproxy_store::entity::resource::resource_binding::ActiveModel {
                id: sea_orm::Set(id.clone()),
                expires_at_ms: sea_orm::Set(Some(1)),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    assert!(h.core.read_publication(&id).await.unwrap().is_none());
    assert!(!h.core.delete_publication(&id).await.unwrap());
}

/// The backend losing an object behind core's back reads as `None`, and
/// deleting it is still a clean tombstone; any other backend failure is
/// `CoreError::File` with the opendal error intact.
#[tokio::test]
async fn backend_failures_surface_as_core_file_errors() {
    let (h, dir) = with_links(Some("https://files.example")).await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let published = r
        .publish(&s, "op-url", PublicationKind::Url, metadata(), body("x"))
        .await
        .unwrap();
    let id = published.handle.binding_id.clone();
    let object = dir.path().join("publications").join(&id);
    assert!(h.core.read_publication(&id).await.unwrap().is_some());

    // Removed behind core's back: NotFound is not an error.
    std::fs::remove_file(&object).unwrap();
    assert!(h.core.read_publication(&id).await.unwrap().is_none());

    // The stored path is now a directory: the fs backend fails to read it
    // with something other than NotFound, and that reaches the host as is.
    std::fs::create_dir(&object).unwrap();
    let error = match h.core.read_publication(&id).await {
        Err(error) => error,
        Ok(read) => panic!("expected an error, got {:?}", read.map(|p| p.id)),
    };
    let gproxy_core::CoreError::File(inner) = &error else {
        panic!("expected CoreError::File, got {error:?}");
    };
    assert_ne!(inner.kind(), gproxy_file::ErrorKind::NotFound);
    assert!(error.to_string().starts_with("file storage: "), "{error}");

    // Delete tombstones the row first; a non-empty directory in the object's
    // place then makes the backend's delete fail the same way (the fs
    // backend removes an empty directory happily).
    std::fs::write(object.join("child"), b"x").unwrap();
    let error = h.core.delete_publication(&id).await.unwrap_err();
    assert!(
        matches!(error, gproxy_core::CoreError::File(_)),
        "{error:?}"
    );
    assert!(h.core.read_publication(&id).await.unwrap().is_none());
    std::fs::remove_dir_all(&object).unwrap();
    // Already tombstoned: nothing to delete and no backend call.
    assert!(!h.core.delete_publication(&id).await.unwrap());
}

#[tokio::test]
async fn publish_is_idempotent_by_operation_id_and_reads_back_the_stored_body() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");

    assert_eq!(
        r.publication_status(&s, "op-1").await.unwrap(),
        PublicationStatus::Missing
    );
    let first = r
        .publish(
            &s,
            "op-1",
            PublicationKind::Id,
            metadata(),
            body("png-bytes"),
        )
        .await
        .unwrap();
    assert_eq!(first.metadata.length, Some(9));
    assert_eq!(first.metadata.filename.as_deref(), Some("out.png"));
    let ResourceReference::Id(id) = &first.reference else {
        panic!("id reference")
    };
    assert_eq!(
        first.handle,
        PublishedHandle {
            binding_id: id.clone()
        }
    );

    // A duplicate with different metadata returns the original and never
    // touches the new body.
    let mut replacement = metadata();
    replacement.filename = Some("other.png".into());
    replacement.expires_at = None;
    let again = r
        .publish(
            &s,
            "op-1",
            PublicationKind::Id,
            replacement,
            body("ignored"),
        )
        .await
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(
        r.publication_status(&s, "op-1").await.unwrap(),
        PublicationStatus::Published(first.clone())
    );

    let read = r.read(&s, &first.reference).await.unwrap();
    assert_eq!(read.metadata, first.metadata);
    assert_eq!(support::read(read.body).await, "png-bytes");
    assert_eq!(
        r.resolve(&s, &first.reference).await.unwrap(),
        first.metadata
    );

    // Store rows: one publication binding with a file object behind it.
    let bindings = h
        .core
        .store()
        .resource_bindings()
        .get_many(std::slice::from_ref(id))
        .await
        .unwrap();
    let row = bindings[0].clone().unwrap();
    assert_eq!(row.kind, "publication");
    assert_eq!(row.public_id, "op-1");
    assert_eq!(row.provider_id, "p");
    let file = h
        .core
        .store()
        .file_objects()
        .get_many(&[row.file_id.clone().unwrap()])
        .await
        .unwrap()[0]
        .clone()
        .unwrap();
    assert_eq!(file.size_bytes, 9);
    assert_eq!(file.object_key, format!("publications/{id}"));
    // Nothing went upstream.
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn preflight_rejections_leave_no_row_and_url_kind_needs_a_link_builder() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");

    // Without a `PublicationUrl` the URL kind is refused before side effects.
    let url = r
        .publish(&s, "url", PublicationKind::Url, metadata(), body("x"))
        .await
        .unwrap_err();
    assert_eq!(url.kind(), CapabilityErrorKind::Unsupported);
    assert!(url.to_string().contains("PublicationUrl"), "{url}");

    let mut none = metadata();
    none.expires_at = None;
    let err = r
        .publish(&s, "none", PublicationKind::Id, none, body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Invalid);

    let mut past = metadata();
    past.expires_at = Some(SystemTime::now() - Duration::from_secs(1));
    let err = r
        .publish(&s, "past", PublicationKind::Id, past, body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Invalid);

    for id in ["url", "none", "past"] {
        assert_eq!(
            r.publication_status(&s, id).await.unwrap(),
            PublicationStatus::Missing
        );
    }

    // URL references pass the fetch policy first; the default one refuses a
    // loopback name before any lookup or connection.
    let err = r
        .resolve(&s, &ResourceReference::Url("http://localhost/a".into()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Unsupported);
}

#[tokio::test]
async fn without_file_storage_publish_is_unsupported_before_side_effects() {
    let h = harness(full(), "round_robin").await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let err = r
        .publish(&s, "op", PublicationKind::Id, metadata(), body("x"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Unsupported);
    assert_eq!(
        r.publication_status(&s, "op").await.unwrap(),
        PublicationStatus::Missing
    );
}

#[tokio::test]
async fn kind_conflict_release_tombstone_and_scope_isolation() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let owner = scope(&h, "tenant-a", "p");
    let other_tenant = scope(&h, "tenant", "p");
    let other_provider = scope(&h, "tenant-a", "claude");

    let published = r
        .publish(&owner, "op", PublicationKind::Id, metadata(), body("abc"))
        .await
        .unwrap();
    let err = r
        .publish(&owner, "op", PublicationKind::Url, metadata(), body("abc"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Conflict);

    // Another scope or another provider does not see the publication; the
    // id falls through to an upstream lookup, which the script answers 404.
    for foreign in [&other_tenant, &other_provider] {
        h.script(vec![json_reply(
            StatusCode::NOT_FOUND,
            json!({"error": "missing"}),
        )]);
        let err = r.resolve(foreign, &published.reference).await.unwrap_err();
        assert_eq!(err.kind(), CapabilityErrorKind::NotFound);
        assert_eq!(
            r.publication_status(foreign, "op").await.unwrap(),
            PublicationStatus::Missing
        );
        let err = r.release(foreign, &published.handle).await.unwrap_err();
        assert_eq!(err.kind(), CapabilityErrorKind::Invalid);
    }

    r.release(&owner, &published.handle).await.unwrap();
    assert_eq!(
        r.publication_status(&owner, "op").await.unwrap(),
        PublicationStatus::Expired
    );
    let err = r.release(&owner, &published.handle).await.unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Expired);
    let err = r.resolve(&owner, &published.reference).await.unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Expired);
    let err = r
        .publish(&owner, "op", PublicationKind::Id, metadata(), body("again"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Expired);
    // The stored object is gone with the tombstone.
    let row = h
        .core
        .store()
        .resource_bindings()
        .get_many(std::slice::from_ref(&published.handle.binding_id))
        .await
        .unwrap()[0]
        .clone()
        .unwrap();
    let missing = h
        .core
        .file_storage()
        .unwrap()
        .exists(&format!("publications/{}", row.id))
        .await
        .unwrap();
    assert!(!missing);
}

#[tokio::test]
async fn stored_expiry_turns_into_a_tombstone_without_a_read() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let published = r
        .publish(&s, "short", PublicationKind::Id, metadata(), body("abc"))
        .await
        .unwrap();
    // Backdate the row's expiry the way time passing would.
    h.core
        .store()
        .resource_bindings()
        .update_many(vec![
            gproxy_store::entity::resource::resource_binding::ActiveModel {
                id: sea_orm::Set(published.handle.binding_id.clone()),
                expires_at_ms: sea_orm::Set(Some(1)),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    let err = r
        .publish(&s, "short", PublicationKind::Id, metadata(), body("new"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Expired);
    assert_eq!(
        r.publication_status(&s, "short").await.unwrap(),
        PublicationStatus::Expired
    );
    let err = r.read(&s, &published.reference).await.unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Expired);
}

#[tokio::test]
async fn cancelled_publish_stays_pending_and_blocks_duplicates() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    // A body that never yields: the publish inserts its row, then parks.
    let stalled = HttpBody::Stream(Box::pin(futures_util::stream::pending()));
    let mut publish = Box::pin(r.publish(&s, "slow", PublicationKind::Id, metadata(), stalled));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut publish)
            .await
            .is_err()
    );
    drop(publish);
    assert_eq!(
        r.publication_status(&s, "slow").await.unwrap(),
        PublicationStatus::Pending
    );
    let err = r
        .publish(&s, "slow", PublicationKind::Id, metadata(), body("dup"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Conflict);
}

#[tokio::test]
async fn body_limit_failure_tombstones_the_publication() {
    let (h, _dir) = with_storage().await;
    let limits = ExecutionLimits::default();
    let mut caps = limits.capability(None);
    caps.write_bytes = 4;
    let r = Resources::new(&h.core, caps, limits.codec());
    let s = scope(&h, "tenant", "p");
    let err = r
        .publish(
            &s,
            "big",
            PublicationKind::Id,
            metadata(),
            body("too large"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Limit);
    assert_eq!(
        r.publication_status(&s, "big").await.unwrap(),
        PublicationStatus::Expired
    );
}

#[tokio::test]
async fn upstream_ids_resolve_and_read_through_the_scope_provider() {
    let h = harness(full(), "round_robin").await;
    // Uploaded by this scope through the gateway: `file-1` on the pool's
    // second credential, so every read must go out as `kb`.
    h.own("p", "b", gproxy_core::owned::FILE_KIND, "file-1")
        .await;
    h.own("p", "a", gproxy_core::owned::FILE_KIND, "file-2")
        .await;
    h.own("claude", "cl1", gproxy_core::owned::FILE_KIND, "file_abc")
        .await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "id": "file-1", "object": "file", "bytes": 3, "created_at": 1,
            "filename": "a.txt", "purpose": "user_data", "status": "processed"
        }),
    )]);
    let metadata = r
        .resolve(&s, &ResourceReference::Id("file-1".into()))
        .await
        .unwrap();
    assert_eq!(metadata.filename.as_deref(), Some("a.txt"));
    assert_eq!(metadata.length, Some(3));
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("GET https://up.example/v1/files/file-1 auth=Bearer kb "),
        "{}",
        seen[0]
    );

    h.script(vec![
        json_reply(
            StatusCode::OK,
            json!({
                "id": "file-1", "object": "file", "bytes": 3, "created_at": 1,
                "filename": "a.txt", "purpose": "user_data", "status": "processed"
            }),
        ),
        (
            StatusCode::OK,
            vec![("content-type", "text/plain")],
            vec![Bytes::from_static(b"abc")],
        ),
    ]);
    let read = r
        .read(&s, &ResourceReference::Id("file-1".into()))
        .await
        .unwrap();
    assert_eq!(read.metadata.filename.as_deref(), Some("a.txt"));
    assert_eq!(support::read(read.body).await, "abc");
    let seen = h.client.seen.lines();
    assert!(seen[2].starts_with("GET https://up.example/v1/files/file-1/content auth=Bearer kb "));

    // Claude-native providers use their own metadata shape.
    let claude = scope(&h, "tenant", "claude");
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "id": "file_abc", "type": "file", "created_at": "2025-01-01T00:00:00Z",
            "filename": "b.pdf", "mime_type": "application/pdf", "size_bytes": 10
        }),
    )]);
    let metadata = r
        .resolve(&claude, &ResourceReference::Id("file_abc".into()))
        .await
        .unwrap();
    assert_eq!(metadata.mime.as_deref(), Some("application/pdf"));
    assert_eq!(metadata.length, Some(10));
    assert!(h.client.seen.lines()[3].starts_with("GET https://claude.example/v1/files/file_abc "));

    // Upstream rejections map to capability kinds without inventing bodies.
    h.script(vec![json_reply(
        StatusCode::NOT_FOUND,
        json!({"error": {"message": "no such file"}}),
    )]);
    let err = r
        .resolve(&s, &ResourceReference::Id("file-2".into()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::NotFound);
}

#[tokio::test]
async fn an_upstream_id_the_scope_did_not_upload_is_not_read() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "a", gproxy_core::owned::FILE_KIND, "file-1")
        .await;
    let r = resources(&h);
    // Another caller on the same provider and credentials, and an id nobody
    // uploaded through the gateway: both not found, the upstream never asked.
    for (caller, id) in [("intruder", "file-1"), ("tenant", "file-unknown")] {
        let s = scope(&h, caller, "p");
        let reference = ResourceReference::Id(id.into());
        let err = r.resolve(&s, &reference).await.unwrap_err();
        assert_eq!(err.kind(), CapabilityErrorKind::NotFound, "{caller}/{id}");
        let Err(err) = r.read(&s, &reference).await else {
            panic!("{caller}/{id} must not be read")
        };
        assert_eq!(err.kind(), CapabilityErrorKind::NotFound, "{caller}/{id}");
    }
    assert!(h.client.seen.lines().is_empty());
}

// ---- URL reads under the fetch policy ----------------------------------

use gproxy_core::{
    AllowAllFetchPolicy, AllowlistFetchPolicy, Core, DefaultFetchPolicy, FetchDecision, FetchPolicy,
};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The same seeded engine with another fetch policy. `into_parts` drops file
/// storage and the link builder, which URL reads never touch.
fn with_policy(h: Harness, policy: Arc<dyn FetchPolicy>) -> Harness {
    let Harness {
        core,
        observer,
        client,
        channel,
    } = h;
    let parts = core.into_parts();
    let core = Core::builder(parts.store)
        .cache(parts.cache)
        .observer(parts.observer)
        .secret_codec(parts.codec)
        .channel(channel.clone())
        .unwrap()
        .fetch_policy(policy)
        .snapshot(parts.data)
        .build()
        .unwrap();
    Harness {
        core,
        observer,
        client,
        channel,
    }
}

fn url(raw: &str) -> url::Url {
    url::Url::parse(raw).unwrap()
}

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

#[test]
fn default_fetch_policy_denies_internal_addresses_and_allows_public_ones() {
    let policy = DefaultFetchPolicy;
    let public = url("https://files.example/a.png");
    assert_eq!(
        policy.decide(&public, &[v4(93, 184, 216, 34)]),
        FetchDecision::Allow
    );
    // No resolver (wasm32): a name alone is allowed, a loopback name is not.
    assert_eq!(policy.decide(&public, &[]), FetchDecision::Allow);
    assert!(matches!(
        policy.decide(&url("http://localhost:8080/x"), &[]),
        FetchDecision::Deny(_)
    ));
    assert!(matches!(
        policy.decide(&url("http://svc.localhost/x"), &[]),
        FetchDecision::Deny(_)
    ));
    // One internal answer among the resolved addresses denies the whole URL.
    assert_eq!(
        policy.decide(&public, &[v4(93, 184, 216, 34), v4(10, 0, 0, 5)]),
        FetchDecision::Deny("private address")
    );
    for (raw, reason) in [
        ("http://127.0.0.1/x", "loopback address"),
        ("http://[::1]/x", "loopback address"),
        ("http://10.1.2.3/x", "private address"),
        ("http://172.16.0.9/x", "private address"),
        ("http://192.168.1.1/x", "private address"),
        ("http://[fd00::1]/x", "private address"),
        ("http://169.254.169.254/latest", "link-local address"),
        ("http://[fe80::1]/x", "link-local address"),
        ("http://0.0.0.0/x", "unspecified address"),
        ("http://[::]/x", "unspecified address"),
        ("http://224.0.0.1/x", "multicast address"),
        ("http://[::ffff:127.0.0.1]/x", "loopback address"),
        ("http://[::ffff:192.168.0.1]/x", "private address"),
    ] {
        assert_eq!(
            policy.decide(&url(raw), &[]),
            FetchDecision::Deny(reason),
            "{raw}"
        );
    }
    // Mapped forms arriving from the resolver are checked as IPv4 too.
    let mapped = IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0x0a00, 0x0001));
    assert_eq!(
        policy.decide(&public, &[mapped]),
        FetchDecision::Deny("private address")
    );
    // Only web schemes are fetched, whatever the address.
    assert!(matches!(
        policy.decide(&url("ftp://files.example/a"), &[v4(93, 184, 216, 34)]),
        FetchDecision::Deny(_)
    ));
    assert!(matches!(
        policy.decide(&url("file:///etc/passwd"), &[]),
        FetchDecision::Deny(_)
    ));

    // The allow-list matches exact names and `*.suffix`, nothing else.
    let list = AllowlistFetchPolicy::new(["cdn.example", "*.media.example"]);
    assert_eq!(
        list.decide(&url("https://CDN.example/a"), &[]),
        FetchDecision::Allow
    );
    assert_eq!(
        list.decide(&url("https://a.b.media.example/a"), &[v4(10, 0, 0, 1)]),
        FetchDecision::Allow
    );
    assert!(matches!(
        list.decide(&url("https://media.example/a"), &[]),
        FetchDecision::Deny(_)
    ));
    assert!(matches!(
        list.decide(&url("https://notcdn.example/a"), &[]),
        FetchDecision::Deny(_)
    ));
    assert!(matches!(
        list.decide(&url("ftp://cdn.example/a"), &[]),
        FetchDecision::Deny(_)
    ));
    assert_eq!(
        AllowAllFetchPolicy.decide(&url("http://127.0.0.1/x"), &[]),
        FetchDecision::Allow
    );
    assert!(matches!(
        AllowAllFetchPolicy.decide(&url("gopher://127.0.0.1/x"), &[]),
        FetchDecision::Deny(_)
    ));
}

/// One scripted HTTP/1.1 reply of the local server.
struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    /// Omit `Content-Length` and end the body by closing the connection.
    close_delimited: bool,
}

fn ok(mime: &str, body: &[u8]) -> Reply {
    Reply {
        status: 200,
        headers: vec![("content-type", mime.to_owned())],
        body: body.to_vec(),
        close_delimited: false,
    }
}

fn redirect(location: &str) -> Reply {
    Reply {
        status: 302,
        headers: vec![("location", location.to_owned())],
        body: Vec::new(),
        close_delimited: false,
    }
}

/// A one-request-per-connection HTTP/1.1 server on 127.0.0.1 that answers
/// its scripted replies in order (404 afterwards) and records request lines.
struct LocalServer {
    addr: SocketAddr,
    hits: Arc<Mutex<Vec<String>>>,
}

impl LocalServer {
    async fn start(replies: Vec<Reply>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let replies = Arc::new(Mutex::new(std::collections::VecDeque::from(replies)));
        let seen = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    let Ok(n) = socket.read(&mut buf).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    head.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&head)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned();
                seen.lock().unwrap().push(line);
                let reply = replies.lock().unwrap().pop_front().unwrap_or(Reply {
                    status: 404,
                    headers: Vec::new(),
                    body: Vec::new(),
                    close_delimited: false,
                });
                let mut out = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", reply.status);
                for (name, value) in &reply.headers {
                    out.push_str(&format!("{name}: {value}\r\n"));
                }
                if !reply.close_delimited {
                    out.push_str(&format!("content-length: {}\r\n", reply.body.len()));
                }
                out.push_str("\r\n");
                let _ = socket.write_all(out.as_bytes()).await;
                let _ = socket.write_all(&reply.body).await;
                let _ = socket.shutdown().await;
            }
        });
        Self { addr, hits }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    fn hits(&self) -> Vec<String> {
        self.hits.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn allow_all_policy_reads_a_url_with_its_metadata() {
    let server = LocalServer::start(vec![
        Reply {
            status: 200,
            headers: vec![
                ("content-type", "image/png; charset=binary".into()),
                (
                    "content-disposition",
                    "attachment; filename=\"plain.png\"; filename*=UTF-8''sm%C3%B6rg%C3%A5s.png"
                        .into(),
                ),
            ],
            body: b"png-bytes".to_vec(),
            close_delimited: false,
        },
        ok("text/plain", b"second"),
    ])
    .await;
    let h = with_policy(
        harness(full(), "round_robin").await,
        Arc::new(AllowAllFetchPolicy),
    );
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let reference = ResourceReference::Url(server.url("/a.png"));

    // `resolve` is the policy check alone: nothing is fetched.
    let metadata = r.resolve(&s, &reference).await.unwrap();
    assert_eq!(
        metadata,
        ResourceMetadata {
            mime: None,
            length: None,
            filename: None,
            expires_at: None,
        }
    );
    assert!(server.hits().is_empty());

    let read = r.read(&s, &reference).await.unwrap();
    assert_eq!(read.metadata.mime.as_deref(), Some("image/png"));
    assert_eq!(read.metadata.filename.as_deref(), Some("smörgås.png"));
    assert_eq!(read.metadata.length, Some(9));
    assert_eq!(read.metadata.expires_at, None);
    assert_eq!(support::read(read.body).await, "png-bytes");
    assert_eq!(server.hits(), vec!["GET /a.png HTTP/1.1"]);
    // The anonymous fetch never went through the scope's credential client.
    assert!(h.client.seen.lines().is_empty());

    let read = r
        .read(&s, &ResourceReference::Url(server.url("/second")))
        .await
        .unwrap();
    assert_eq!(support::read(read.body).await, "second");
    // A non-2xx answer is a rejection, not a body.
    let err = r
        .read(&s, &ResourceReference::Url(server.url("/missing")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::NotFound);
    let err = r
        .read(&s, &ResourceReference::Url("not a url".into()))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Invalid);
}

#[tokio::test]
async fn default_policy_denies_a_loopback_url_before_any_connection() {
    let server = LocalServer::start(vec![ok("text/plain", b"never")]).await;
    let h = harness(full(), "round_robin").await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");
    let reference = ResourceReference::Url(server.url("/a"));
    for result in [
        r.read(&s, &reference).await.map(|_| ()),
        r.resolve(&s, &reference).await.map(|_| ()),
    ] {
        let err = result.unwrap_err();
        assert_eq!(err.kind(), CapabilityErrorKind::Unsupported);
        assert!(err.to_string().contains("loopback address"), "{err}");
    }
    assert!(server.hits().is_empty());
}

/// Trusts the local test server itself and defers to the default policy for
/// everything else, so a redirect off the server is judged by the default.
struct LocalThenDefault(SocketAddr);
impl FetchPolicy for LocalThenDefault {
    fn decide(&self, url: &url::Url, resolved: &[IpAddr]) -> FetchDecision {
        if url.host_str() == Some(&self.0.ip().to_string()) && url.port() == Some(self.0.port()) {
            FetchDecision::Allow
        } else {
            DefaultFetchPolicy.decide(url, resolved)
        }
    }
}

#[tokio::test]
async fn redirects_are_followed_under_the_policy_and_bounded() {
    let server = LocalServer::start(vec![
        redirect("/hop"),
        ok("text/plain", b"landed"),
        redirect("http://192.168.0.1/internal"),
        redirect("/1"),
        redirect("/2"),
        redirect("/3"),
        redirect("/4"),
    ])
    .await;
    let h = with_policy(
        harness(full(), "round_robin").await,
        Arc::new(LocalThenDefault(server.addr)),
    );
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");

    // A relative redirect to the same allowed origin is followed.
    let read = r
        .read(&s, &ResourceReference::Url(server.url("/start")))
        .await
        .unwrap();
    assert_eq!(support::read(read.body).await, "landed");
    assert_eq!(
        server.hits(),
        vec!["GET /start HTTP/1.1", "GET /hop HTTP/1.1"]
    );

    // A redirect onto a private address is refused mid-chain, unconnected.
    let err = r
        .read(&s, &ResourceReference::Url(server.url("/bounce")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Unsupported);
    assert!(err.to_string().contains("private address"), "{err}");
    assert_eq!(server.hits().len(), 3);

    // More than three hops is a transport failure.
    let err = r
        .read(&s, &ResourceReference::Url(server.url("/loop")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Transport);
    assert!(err.to_string().contains("redirected"), "{err}");
    assert_eq!(server.hits().len(), 7);
}

#[tokio::test]
async fn url_reads_are_bounded_by_the_read_limit() {
    let server = LocalServer::start(vec![
        ok("text/plain", b"too large"),
        Reply {
            status: 200,
            headers: vec![("content-type", "text/plain".into())],
            body: b"too large".to_vec(),
            close_delimited: true,
        },
        ok("text/plain", b"ok"),
    ])
    .await;
    let h = with_policy(
        harness(full(), "round_robin").await,
        Arc::new(AllowAllFetchPolicy),
    );
    let limits = ExecutionLimits::default();
    let mut caps = limits.capability(None);
    caps.read_bytes = 4;
    let r = Resources::new(&h.core, caps, limits.codec());
    let s = scope(&h, "tenant", "p");

    // Declared up front: refused before the body is read.
    let err = r
        .read(&s, &ResourceReference::Url(server.url("/declared")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Limit);
    // Undeclared: refused while reading.
    let err = r
        .read(&s, &ResourceReference::Url(server.url("/streamed")))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), CapabilityErrorKind::Limit);
    let read = r
        .read(&s, &ResourceReference::Url(server.url("/small")))
        .await
        .unwrap();
    assert_eq!(read.metadata.length, Some(2));
    assert_eq!(support::read(read.body).await, "ok");
}
