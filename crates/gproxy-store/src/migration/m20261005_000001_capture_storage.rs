//! Add encoded capture payloads and tenant-scoped blobs; backfill headers in
//! bounded pages. Legacy payloads stay identity encoded and are pruned normally.
use crate::{
    capture::{canonical_headers, header_hash},
    entity::{
        config::setting,
        usage::{
            capture_blob, capture_body, capture_body_blob, downstream_event, downstream_record,
            header_set, upstream_event, upstream_record,
        },
    },
};
use gproxy_seaorm::{
    SchemaProbeExt,
    sea_orm_migration::{MigrationName, MigrationTrait, SchemaManager},
};
use sea_orm::{
    ColumnTrait, Condition, DbErr, EntityName, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Schema, Set,
    sea_query::{OnConflict, Table},
};

pub struct Migration;
impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261005_000001_capture_storage"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());
        macro_rules! create {
            ($entity:ident) => {{
                let table = $entity::Entity.table_name();
                if !manager.probe_table(table).await? {
                    manager
                        .create_table(schema.create_table_from_entity($entity::Entity))
                        .await?;
                }
                for index in schema.create_index_from_entity($entity::Entity) {
                    let name = index.get_index_spec().get_name().unwrap().to_owned();
                    if !manager.probe_index(table, &name).await? {
                        manager.create_index(index).await?;
                    }
                }
            }};
        }
        create!(header_set);
        create!(capture_blob);
        create!(capture_body);
        create!(capture_body_blob);
        macro_rules! columns {
            ($entity:ident, [$($name:literal),*]) => {{
                let table = $entity::Entity.table_name();
                let definition = schema.create_table_from_entity($entity::Entity);
                for name in [$($name),*] {
                    if !manager.probe_column(table, name).await? {
                        let column = definition.get_columns().iter().find(|c| c.get_column_name() == name)
                            .ok_or_else(|| DbErr::Migration(format!("missing entity column {table}.{name}")))?;
                        let mut column = column.clone();
                        if name.ends_with("_headers_hash") {
                            // SQLite cannot add a table constraint later, but a
                            // nullable ADD COLUMN may carry REFERENCES. Postgres
                            // accepts the same inline reference.
                            column.extra("REFERENCES header_sets(hash) ON DELETE RESTRICT");
                        }
                        manager.alter_table(Table::alter().table(table).add_column(column).to_owned()).await?;
                    }
                }
                for index in schema.create_index_from_entity($entity::Entity) {
                    let name = index.get_index_spec().get_name().unwrap().to_owned();
                    if !manager.probe_index(table, &name).await? { manager.create_index(index).await?; }
                }
            }};
        }
        columns!(
            upstream_record,
            [
                "request_headers_hash",
                "response_headers_hash",
                "request_body_encoding",
                "response_body_encoding",
                "request_body_id"
            ]
        );
        columns!(
            downstream_record,
            [
                "request_headers_hash",
                "response_headers_hash",
                "request_body_encoding",
                "response_body_encoding",
                "request_body_id"
            ]
        );
        columns!(upstream_event, ["encoding", "chunk_offsets", "body_id"]);
        columns!(downstream_event, ["encoding", "chunk_offsets", "body_id"]);
        columns!(
            setting,
            ["capture_payload_retention_days", "capture_payload_max_mb"]
        );
        macro_rules! backfill {
            ($entity:ident) => {{
                loop {
                    let rows: Vec<(String, Option<serde_json::Value>, Option<serde_json::Value>)> =
                        $entity::Entity::find()
                            .select_only()
                            .column($entity::Column::Id)
                            .column($entity::Column::RequestHeaders)
                            .column($entity::Column::ResponseHeaders)
                            .filter(
                                Condition::any()
                                    .add($entity::Column::RequestHeaders.is_not_null())
                                    .add($entity::Column::ResponseHeaders.is_not_null()),
                            )
                            .order_by_asc($entity::Column::Id)
                            .limit(128)
                            .into_tuple()
                            .all(manager.get_connection())
                            .await?;
                    if rows.is_empty() {
                        break;
                    }
                    for (id, request, response) in rows {
                        let mut patch = $entity::ActiveModel {
                            id: Set(id),
                            ..Default::default()
                        };
                        for (value, inline, hash) in [
                            (
                                request,
                                &mut patch.request_headers,
                                &mut patch.request_headers_hash,
                            ),
                            (
                                response,
                                &mut patch.response_headers,
                                &mut patch.response_headers_hash,
                            ),
                        ] {
                            if let Some(value) = value {
                                let value = canonical_headers(&value);
                                let key = header_hash(&value);
                                header_set::Entity::insert(header_set::ActiveModel {
                                    hash: Set(key.clone()),
                                    headers: Set(value),
                                })
                                .on_conflict(
                                    OnConflict::column(header_set::Column::Hash)
                                        .do_nothing()
                                        .to_owned(),
                                )
                                .try_insert()
                                .exec(manager.get_connection())
                                .await?;
                                *inline = Set(None);
                                *hash = Set(Some(key));
                            }
                        }
                        $entity::Entity::update(patch)
                            .exec(manager.get_connection())
                            .await?;
                    }
                }
            }};
        }
        backfill!(upstream_record);
        backfill!(downstream_record);
        Ok(())
    }
    // Reverting would discard payloads; older builds must not read this schema.
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
