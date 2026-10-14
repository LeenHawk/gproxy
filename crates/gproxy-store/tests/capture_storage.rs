#![cfg(not(target_arch = "wasm32"))]

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Store,
    capture::{Chunk, PayloadRetention, Segments, compress, decompress, tenant_scope},
    entity::usage::{
        capture_blob, capture_body_blob,
        capture_event::{CaptureDirection as D, CaptureEventKind as K},
        capture_link, downstream_record as record, header_set, upstream_event, upstream_record,
    },
};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, EntityTrait, Set};
use serde_json::json;

async fn fixture() -> Store<DatabaseConnection> {
    // Optional live-backend run against a disposable PostgreSQL database:
    // cargo test -p gproxy-store --features sea-orm/sqlx-postgres --test capture_storage
    // Each fixture uses its own schema, leaving other schemas untouched.
    let postgres = std::env::var("GPROXY_CAPTURE_TEST_POSTGRES_URL").ok();
    let mut options =
        ConnectOptions::new(postgres.clone().unwrap_or_else(|| "sqlite::memory:".into()));
    options.max_connections(1).sqlx_logging(false);
    if postgres.is_some() {
        let mut nonce = [0u8; 8];
        getrandom::fill(&mut nonce).unwrap();
        let schema = format!("capture_test_{}", u64::from_le_bytes(nonce));
        let db = Database::connect(options.clone()).await.unwrap();
        db.execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        db.close().await.unwrap();
        options.set_schema_search_path(schema);
    }
    let store = Store::new(Database::connect(options).await.unwrap());
    store.sync().await.unwrap();
    store
}
fn row(id: &str, ended: i64) -> record::ActiveModel {
    record::ActiveModel {
        id: Set(id.into()),
        kind: Set(record::CaptureKind::Http),
        started_at_ms: Set(ended - 1),
        ended_at_ms: Set(Some(ended)),
        state: Set(record::CaptureState::Completed),
        api_key_id: Set(Some("tenant-a".into())),
        request_body_state: Set(record::CaptureBodyState::Complete),
        response_body_state: Set(record::CaptureBodyState::Complete),
        ..Default::default()
    }
}
async fn write(store: &Store<DatabaseConnection>, row: record::ActiveModel) {
    store
        .connection()
        .atomic_batch_owned(store.capture_downstream_head(row, false).unwrap())
        .await
        .unwrap();
}
async fn hydrated(
    store: &Store<DatabaseConnection>,
    id: &str,
) -> gproxy_store::entity::usage::capture_record::Model {
    let row = store
        .downstream_records()
        .get_many(&[id.into()])
        .await
        .unwrap()
        .pop()
        .flatten()
        .unwrap();
    store.hydrate_capture(row.into()).await.unwrap()
}
fn data(size: usize) -> Vec<u8> {
    let mut n = 0xabcdef1234567890u64;
    (0..size)
        .map(|_| {
            n ^= n << 13;
            n ^= n >> 7;
            n ^= n << 17;
            n as u8
        })
        .collect()
}

#[test]
fn zstd_round_trip_and_identity_for_incompressible_bytes() {
    let bytes = b"data: {\"delta\":\"hello world\"}\n\n".repeat(4000);
    let (encoding, compressed) = compress(&bytes).unwrap();
    assert_eq!(encoding, "zstd");
    assert!(compressed.len() < bytes.len() / 20);
    eprintln!(
        "synthetic repeated SSE: {} -> {} encoded bytes",
        bytes.len(),
        compressed.len()
    );
    assert_eq!(decompress(&encoding, &compressed).unwrap(), bytes);
    let bytes = data(65_536);
    let (encoding, encoded) = compress(&bytes).unwrap();
    assert_eq!(decompress(&encoding, &encoded).unwrap(), bytes);
    assert_eq!(decompress("identity", b"legacy").unwrap(), b"legacy");
    assert!(decompress("unknown", b"").is_err());
}

#[tokio::test]
async fn segments_restore_interleaved_directions_empty_chunks_and_frames() {
    let store = fixture().await;
    store
        .upstream_records()
        .insert_many(vec![upstream_record::ActiveModel {
            id: Set("u".into()),
            kind: Set(record::CaptureKind::Http),
            started_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    let mut chunks = Vec::new();
    for sequence in 0..40 {
        chunks.push(Chunk {
            sequence,
            turn_id: None,
            direction: if sequence % 3 == 0 {
                D::Request
            } else {
                D::Response
            },
            kind: if sequence < 30 { K::Bytes } else { K::WsText },
            payload: if sequence == 5 {
                vec![]
            } else {
                format!("data: {{\"delta\":{sequence}}}\n\n")
                    .repeat(300)
                    .into_bytes()
            },
            // Coalesced chunks report their segment head's arrival time.
            observed_at_ms: 100,
        });
    }
    let mut segments = Segments::default();
    let mut statements = Vec::new();
    for chunk in chunks.clone() {
        for segment in segments.push(chunk) {
            statements.extend(
                store
                    .capture_upstream_segment("u", "api_key:a", segment)
                    .unwrap(),
            );
        }
    }
    for segment in segments.drain(true) {
        statements.extend(
            store
                .capture_upstream_segment("u", "api_key:a", segment)
                .unwrap(),
        );
    }
    store
        .connection()
        .atomic_batch_owned(statements)
        .await
        .unwrap();
    let physical = store
        .upstream_events()
        .query(upstream_event::Entity::find())
        .await
        .unwrap();
    assert!(physical.len() < chunks.len() / 2);
    let decoded = store
        .hydrate_capture_events(physical.into_iter().map(Into::into).collect())
        .await
        .unwrap();
    let decoded: Vec<_> = decoded
        .into_iter()
        .map(|e| Chunk {
            sequence: e.sequence,
            turn_id: e.turn_id,
            direction: e.direction,
            kind: e.kind,
            payload: e.payload,
            observed_at_ms: e.observed_at_ms,
        })
        .collect();
    assert_eq!(decoded, chunks);
}

#[tokio::test]
async fn canonical_header_sets_deduplicate_preserving_repeated_values() {
    let store = fixture().await;
    for (id, headers) in [
        (
            "a",
            json!([["X-Z", "last"], ["x-a", "one"], ["x-a", "two"]]),
        ),
        (
            "b",
            json!([["x-a", "one"], ["x-a", "two"], ["x-z", "last"]]),
        ),
    ] {
        let mut row = row(id, 1);
        row.request_headers = Set(Some(headers.clone()));
        row.response_headers = Set(Some(headers));
        write(&store, row).await;
    }
    let sets = store
        .header_sets()
        .query(header_set::Entity::find())
        .await
        .unwrap();
    assert_eq!(sets.len(), 1);
    let physical = store
        .downstream_records()
        .get_many(&["a".into()])
        .await
        .unwrap()
        .pop()
        .flatten()
        .unwrap();
    assert_eq!(physical.request_headers, None);
    assert_eq!(physical.request_headers_hash, Some(sets[0].hash.clone()));
    assert_eq!(
        hydrated(&store, "a").await.request_headers,
        Some(json!([["x-a", "one"], ["x-a", "two"], ["x-z", "last"]]))
    );
}

#[tokio::test]
async fn cdc_shares_prefix_and_both_sides_but_never_another_tenant_and_gc_reclaims_last_link() {
    let store = fixture().await;
    let prefix = data(300_000);
    let mut first = prefix.clone();
    first.extend_from_slice(b"first tail");
    let mut second = prefix.clone();
    second.extend_from_slice(b"second longer tail");
    for (id, bytes) in [("a", &first), ("b", &second)] {
        let mut row = row(id, 1);
        row.request_body = Set(Some(bytes.clone()));
        write(&store, row).await;
    }
    let blobs = store
        .capture_blobs()
        .query(capture_blob::Entity::find())
        .await
        .unwrap();
    let links = store
        .capture_body_blobs()
        .query(capture_body_blob::Entity::find())
        .await
        .unwrap();
    assert!(
        links.len() > blobs.len() * 3 / 2,
        "most CDC blobs must be reused across the common prefix"
    );
    eprintln!(
        "synthetic CDC pair: {} raw bytes, {} unique blob bytes, {} hash-reference bytes",
        first.len() + second.len(),
        blobs.iter().map(|b| b.payload.len()).sum::<usize>(),
        links.len() * 64
    );
    assert_eq!(
        hydrated(&store, "a").await.request_body,
        Some(first.clone())
    );
    assert_eq!(hydrated(&store, "b").await.request_body, Some(second));
    let before = blobs.len();
    let up = upstream_record::ActiveModel {
        id: Set("up".into()),
        api_key_id: Set(Some("tenant-a".into())),
        kind: Set(record::CaptureKind::Http),
        started_at_ms: Set(1),
        request_body: Set(Some(first.clone())),
        ..Default::default()
    };
    store
        .connection()
        .atomic_batch_owned(store.capture_upstream_head(up, false).unwrap())
        .await
        .unwrap();
    assert_eq!(
        store
            .capture_blobs()
            .query(capture_blob::Entity::find())
            .await
            .unwrap()
            .len(),
        before
    );
    let mut other = row("other", 1);
    other.api_key_id = Set(Some("tenant-b".into()));
    other.request_body = Set(Some(first.clone()));
    write(&store, other).await;
    assert!(
        store
            .capture_blobs()
            .query(capture_blob::Entity::find())
            .await
            .unwrap()
            .len()
            > before
    );
    assert_ne!(tenant_scope(None, None, "a"), tenant_scope(None, None, "b"));
    store
        .downstream_records()
        .delete_many(&["a".into(), "b".into(), "other".into()])
        .await
        .unwrap();
    store.collect_capture_garbage().await.unwrap();
    assert!(
        !store
            .capture_blobs()
            .query(capture_blob::Entity::find())
            .await
            .unwrap()
            .is_empty()
    );
    let up = store
        .upstream_records()
        .get_many(&["up".into()])
        .await
        .unwrap()
        .pop()
        .flatten()
        .unwrap();
    assert_eq!(
        store.hydrate_capture(up.into()).await.unwrap().request_body,
        Some(first)
    );
    store
        .upstream_records()
        .delete_many(&["up".into()])
        .await
        .unwrap();
    store.collect_capture_garbage().await.unwrap();
    assert!(
        store
            .capture_blobs()
            .query(capture_blob::Entity::find())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn retention_prunes_payloads_by_age_then_size_keeps_metadata_links_and_active_captures() {
    let store = fixture().await;
    let day = 86_400_000;
    for (id, ended) in [("old", day), ("recent", 9 * day), ("new", 10 * day)] {
        let mut row = row(id, ended);
        row.request_body = Set(Some(data(8192)));
        row.response_body = Set(Some(data(20000)));
        row.request_headers = Set(Some(json!([["x-client", "test"]])));
        write(&store, row).await;
    }
    let mut active = row("active", day);
    active.state = Set(record::CaptureState::InProgress);
    active.response_body = Set(Some(data(1000)));
    write(&store, active).await;
    store
        .capture_links()
        .insert_many(vec![capture_link::ActiveModel {
            downstream_id: Set("old".into()),
            upstream_id: Set("independent".into()),
        }])
        .await
        .unwrap();
    assert_eq!(
        store
            .prune_capture_payloads(
                PayloadRetention {
                    days: Some(7),
                    max_bytes: None
                },
                10 * day
            )
            .await
            .unwrap(),
        1
    );
    let old = hydrated(&store, "old").await;
    assert_eq!(old.request_body, None);
    assert_eq!(old.response_body, None);
    assert!(old.request_headers.is_some());
    assert_eq!(
        store
            .capture_links()
            .query(capture_link::Entity::find())
            .await
            .unwrap()
            .len(),
        1
    );
    let bytes = store.capture_payload_bytes().await.unwrap();
    assert_eq!(
        store
            .prune_capture_payloads(
                PayloadRetention {
                    days: None,
                    max_bytes: Some(bytes - 1)
                },
                10 * day
            )
            .await
            .unwrap(),
        1
    );
    assert_eq!(hydrated(&store, "recent").await.response_body, None);
    assert!(hydrated(&store, "new").await.response_body.is_some());
    store
        .prune_capture_payloads(
            PayloadRetention {
                days: Some(0),
                max_bytes: Some(0),
            },
            10 * day,
        )
        .await
        .unwrap();
    assert!(hydrated(&store, "active").await.response_body.is_some());
    assert!(
        store
            .capture_blobs()
            .query(capture_blob::Entity::find())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .downstream_records()
            .query(record::Entity::find())
            .await
            .unwrap()
            .len(),
        4
    );
}
