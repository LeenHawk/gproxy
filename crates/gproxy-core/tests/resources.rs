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
async fn preflight_rejections_leave_no_row_and_url_kind_is_unsupported() {
    let (h, _dir) = with_storage().await;
    let r = resources(&h);
    let s = scope(&h, "tenant", "p");

    let url = r
        .publish(&s, "url", PublicationKind::Url, metadata(), body("x"))
        .await
        .unwrap_err();
    assert_eq!(url.kind(), CapabilityErrorKind::Unsupported);

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

    // URL references are never read: core has no allow-list.
    let err = r
        .resolve(&s, &ResourceReference::Url("https://x.example/a".into()))
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
        seen[0].starts_with("GET https://up.example/v1/files/file-1 auth=Bearer "),
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
    assert!(seen[2].starts_with("GET https://up.example/v1/files/file-1/content "));

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
