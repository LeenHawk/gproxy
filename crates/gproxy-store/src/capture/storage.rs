use super::{Chunk, Segment, Segments, compress, decompress, split};
use crate::{
    Result, Store,
    entity::usage::{
        capture_blob, capture_body, capture_body_blob, capture_event, capture_record,
        downstream_event, downstream_record, header_set, upstream_event, upstream_record,
    },
    error::invalid,
};
use gproxy_seaorm::BatchConnectionTrait;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue, ColumnTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, QueryTrait, Set,
    Statement,
};
use serde_json::Value;

/// Sort different names, retaining the relative order of repeated values.
/// Header names are case-insensitive; values and duplicates remain exact.
pub fn canonical_headers(headers: &Value) -> Value {
    let Some(pairs) = headers.as_array() else {
        return headers.clone();
    };
    let mut pairs = pairs.clone();
    for pair in &mut pairs {
        if let Some(name) = pair
            .get(0)
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
        {
            pair[0] = Value::String(name);
        }
    }
    pairs.sort_by(|a, b| {
        a.get(0)
            .and_then(Value::as_str)
            .cmp(&b.get(0).and_then(Value::as_str))
    });
    Value::Array(pairs)
}
pub fn header_hash(headers: &Value) -> String {
    blake3::hash(&serde_json::to_vec(&canonical_headers(headers)).expect("JSON serializes"))
        .to_hex()
        .to_string()
}
/// Unattributed requests get a capture-local scope, never a shared global pool.
pub fn tenant_scope(api_key: Option<&str>, user: Option<&str>, capture: &str) -> String {
    if let Some(key) = api_key {
        format!("api_key:{key}")
    } else if let Some(user) = user {
        format!("user:{user}")
    } else {
        format!("capture:{capture}")
    }
}
fn header_statement(backend: DbBackend, value: &Value) -> (String, Statement) {
    let headers = canonical_headers(value);
    let hash = header_hash(&headers);
    let statement = header_set::Entity::insert(header_set::ActiveModel {
        hash: Set(hash.clone()),
        headers: Set(headers),
    })
    .on_conflict(
        OnConflict::column(header_set::Column::Hash)
            .do_nothing()
            .to_owned(),
    )
    .build(backend);
    (hash, statement)
}
fn active_string(value: &ActiveValue<Option<String>>) -> Option<&str> {
    match value {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => value.as_deref(),
        _ => None,
    }
}

// The two physical capture tables deliberately have the same storage contract.
macro_rules! head_writer {
    ($method:ident, $entity:ident, $repo:ident, $side:expr) => {
        pub fn $method(
            &self,
            mut row: $entity::ActiveModel,
            update: bool,
        ) -> Result<Vec<Statement>> {
            let backend = self.db.get_database_backend();
            let id = row.id.clone().unwrap();
            let mut before = Vec::new();
            let mut after = Vec::new();
            for (inline, hash) in [
                (&mut row.request_headers, &mut row.request_headers_hash),
                (&mut row.response_headers, &mut row.response_headers_hash),
            ] {
                if let ActiveValue::Set(Some(headers)) = inline {
                    let (key, statement) = header_statement(backend, headers);
                    before.push(statement);
                    *hash = Set(Some(key));
                    *inline = Set(None);
                }
            }
            if let ActiveValue::Set(Some(bytes)) = &row.request_body {
                let scope = tenant_scope(
                    active_string(&row.api_key_id),
                    active_string(&row.user_id),
                    &format!("{}:{id}", $side),
                );
                let (body_id, statements) =
                    self.capture_body_statements($side, &id, "inline", &scope, bytes)?;
                after.extend(statements);
                row.request_body_id = Set(Some(body_id));
                row.request_body = Set(None);
                row.request_body_encoding = Set("identity".into());
            }
            if let ActiveValue::Set(Some(bytes)) = &row.response_body {
                let (encoding, bytes) = compress(bytes)?;
                row.response_body = Set(Some(bytes));
                row.response_body_encoding = Set(encoding);
            }
            if update {
                before.extend(self.$repo().update_statement(row)?);
            } else {
                before.push(self.$repo().insert_statement(row)?);
            }
            before.extend(after);
            Ok(before)
        }
    };
}
macro_rules! segment_writer {
    ($method:ident, $entity:ident, $side:expr) => {
        pub fn $method(
            &self,
            capture_id: &str,
            scope: &str,
            segment: Segment,
        ) -> Result<Vec<Statement>> {
            let backend = self.db.get_database_backend();
            let head = segment.head;
            let mut statements = Vec::new();
            let (encoding, payload, body_id) =
                if head.direction == capture_event::CaptureDirection::Request {
                    let (id, body) = self.capture_body_statements(
                        $side,
                        capture_id,
                        &head.sequence.to_string(),
                        scope,
                        &head.payload,
                    )?;
                    statements.extend(body);
                    ("identity".to_owned(), Vec::new(), Some(id))
                } else {
                    let (encoding, payload) = compress(&head.payload)?;
                    (encoding, payload, None)
                };
            statements.push(
                $entity::Entity::insert($entity::ActiveModel {
                    capture_id: Set(capture_id.to_owned()),
                    sequence: Set(head.sequence),
                    turn_id: Set(head.turn_id),
                    direction: Set(head.direction),
                    kind: Set(head.kind),
                    payload: Set(payload),
                    encoding: Set(encoding),
                    body_id: Set(body_id),
                    chunk_offsets: Set(Some(segment.chunk_offsets)),
                    observed_at_ms: Set(head.observed_at_ms),
                })
                .build(backend),
            );
            Ok(statements)
        }
    };
}
impl<C: BatchConnectionTrait> Store<C> {
    head_writer!(
        capture_upstream_head,
        upstream_record,
        upstream_records,
        "upstream"
    );
    head_writer!(
        capture_downstream_head,
        downstream_record,
        downstream_records,
        "downstream"
    );
    segment_writer!(capture_upstream_segment, upstream_event, "upstream");
    segment_writer!(capture_downstream_segment, downstream_event, "downstream");

    fn capture_body_statements(
        &self,
        side: &str,
        capture_id: &str,
        part: &str,
        scope: &str,
        bytes: &[u8],
    ) -> Result<(String, Vec<Statement>)> {
        let backend = self.db.get_database_backend();
        let id = blake3::hash(
            &serde_json::to_vec(&(
                side,
                capture_id,
                part,
                blake3::hash(bytes).to_hex().as_str(),
            ))
            .expect("strings serialize"),
        )
        .to_hex()
        .to_string();
        let scope_key = blake3::derive_key("gproxy capture tenant v1", scope.as_bytes());
        let mut statements = vec![
            capture_body::Entity::insert(capture_body::ActiveModel {
                id: Set(id.clone()),
                upstream_id: Set((side == "upstream").then(|| capture_id.to_owned())),
                downstream_id: Set((side == "downstream").then(|| capture_id.to_owned())),
                raw_size: Set(bytes.len() as i64),
            })
            .on_conflict(
                OnConflict::column(capture_body::Column::Id)
                    .do_nothing()
                    .to_owned(),
            )
            .build(backend),
        ];
        for (ordinal, chunk) in fastcdc::v2020::FastCDC::new(bytes, 2048, 8192, 32768).enumerate() {
            let raw = &bytes[chunk.offset..chunk.offset + chunk.length];
            let hash = blake3::keyed_hash(&scope_key, raw).to_hex().to_string();
            let (encoding, payload) = compress(raw)?;
            statements.push(
                capture_blob::Entity::insert(capture_blob::ActiveModel {
                    id: Set(hash.clone()),
                    payload: Set(payload),
                    encoding: Set(encoding),
                    raw_size: Set(raw.len() as i64),
                })
                .on_conflict(
                    OnConflict::column(capture_blob::Column::Id)
                        .do_nothing()
                        .to_owned(),
                )
                .build(backend),
            );
            statements.push(
                capture_body_blob::Entity::insert(capture_body_blob::ActiveModel {
                    body_id: Set(id.clone()),
                    ordinal: Set(
                        i32::try_from(ordinal).map_err(|_| invalid("too many capture chunks"))?
                    ),
                    blob_id: Set(hash),
                })
                .on_conflict(
                    OnConflict::columns([
                        capture_body_blob::Column::BodyId,
                        capture_body_blob::Column::Ordinal,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .build(backend),
            );
        }
        Ok((id, statements))
    }

    pub fn capture_downstream_events(
        &self,
        capture_id: &str,
        scope: &str,
        events: Vec<downstream_event::ActiveModel>,
    ) -> Result<Vec<Statement>> {
        let mut segments = Segments::default();
        let mut statements = Vec::new();
        for event in events {
            for segment in segments.push(Chunk {
                sequence: event.sequence.unwrap(),
                turn_id: event.turn_id.unwrap(),
                direction: event.direction.unwrap(),
                kind: event.kind.unwrap(),
                payload: event.payload.unwrap(),
                observed_at_ms: event.observed_at_ms.unwrap(),
            }) {
                statements.extend(self.capture_downstream_segment(capture_id, scope, segment)?);
            }
        }
        for segment in segments.drain(true) {
            statements.extend(self.capture_downstream_segment(capture_id, scope, segment)?);
        }
        Ok(statements)
    }

    async fn read_capture_body(&self, id: &str) -> Result<Vec<u8>> {
        let body = self
            .capture_bodies()
            .get_many(&[id.to_owned()])
            .await?
            .pop()
            .flatten()
            .ok_or_else(|| invalid("missing capture body"))?;
        let links = self
            .capture_body_blobs()
            .query(
                capture_body_blob::Entity::find()
                    .filter(capture_body_blob::Column::BodyId.eq(id))
                    .order_by_asc(capture_body_blob::Column::Ordinal),
            )
            .await?;
        let blobs = self
            .capture_blobs()
            .get_many(&links.into_iter().map(|l| l.blob_id).collect::<Vec<_>>())
            .await?;
        let mut bytes = Vec::new();
        for blob in blobs {
            let blob = blob.ok_or_else(|| invalid("missing capture blob"))?;
            let raw = decompress(&blob.encoding, &blob.payload)?;
            if raw.len() as i64 != blob.raw_size {
                return Err(invalid("capture blob length mismatch"));
            }
            bytes.extend(raw);
        }
        if bytes.len() as i64 != body.raw_size {
            return Err(invalid("capture body length mismatch"));
        }
        Ok(bytes)
    }

    /// Resolve physical storage before returning a capture through SDK/app DTOs.
    pub async fn hydrate_capture(
        &self,
        mut row: capture_record::Model,
    ) -> Result<capture_record::Model> {
        for (hash, headers) in [
            (&row.request_headers_hash, &mut row.request_headers),
            (&row.response_headers_hash, &mut row.response_headers),
        ] {
            if let Some(hash) = hash {
                *headers = Some(
                    self.header_sets()
                        .get_many(std::slice::from_ref(hash))
                        .await?
                        .pop()
                        .flatten()
                        .ok_or_else(|| invalid("missing capture header set"))?
                        .headers,
                );
            }
        }
        if let Some(id) = &row.request_body_id {
            row.request_body = Some(self.read_capture_body(id).await?);
        } else if let Some(bytes) = &row.request_body {
            row.request_body = Some(decompress(&row.request_body_encoding, bytes)?);
        }
        if let Some(bytes) = &row.response_body {
            row.response_body = Some(decompress(&row.response_body_encoding, bytes)?);
        }
        row.request_body_encoding = "identity".into();
        row.response_body_encoding = "identity".into();
        Ok(row)
    }

    /// Expand stored segments back to original transport chunks / WS messages.
    pub async fn hydrate_capture_events(
        &self,
        events: Vec<capture_event::Model>,
    ) -> Result<Vec<capture_event::Model>> {
        let mut out = Vec::new();
        for event in events {
            let payload = if let Some(id) = &event.body_id {
                self.read_capture_body(id).await?
            } else {
                decompress(&event.encoding, &event.payload)?
            };
            let chunks = split(
                Chunk {
                    sequence: event.sequence,
                    turn_id: event.turn_id,
                    direction: event.direction,
                    kind: event.kind,
                    payload,
                    observed_at_ms: event.observed_at_ms,
                },
                event.chunk_offsets.as_deref(),
            )?;
            for chunk in chunks {
                out.push(capture_event::Model {
                    capture_id: event.capture_id.clone(),
                    sequence: chunk.sequence,
                    turn_id: chunk.turn_id,
                    direction: chunk.direction,
                    kind: chunk.kind,
                    payload: chunk.payload,
                    observed_at_ms: chunk.observed_at_ms,
                    encoding: "identity".into(),
                    chunk_offsets: None,
                    body_id: None,
                });
            }
        }
        out.sort_by(|a, b| (&a.capture_id, a.sequence).cmp(&(&b.capture_id, b.sequence)));
        Ok(out)
    }
}
