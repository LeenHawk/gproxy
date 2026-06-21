//! Tokenizer vocab ops for the `db` backend: raw `tokenizer.json` BLOBs in
//! the `tokenizer_vocabs` table, upsert-on-put.

use sea_orm::ActiveValue::Set;
use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, EntityTrait, QuerySelect};

use crate::store::persistence::db::entities::tokenize::tokenizer_vocab;

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<String>> {
    Ok(tokenizer_vocab::Entity::find()
        .select_only()
        .column(tokenizer_vocab::Column::Name)
        .into_tuple::<String>()
        .all(conn)
        .await?)
}

pub async fn get(conn: &DatabaseConnection, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    Ok(tokenizer_vocab::Entity::find_by_id(name)
        .one(conn)
        .await?
        .map(|m| m.bytes))
}

pub async fn put(conn: &DatabaseConnection, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    let am = tokenizer_vocab::ActiveModel {
        name: Set(name.to_owned()),
        bytes: Set(bytes.to_vec()),
        updated_at: Set(crate::store::persistence::db::ops::now_secs()),
    };
    tokenizer_vocab::Entity::insert(am)
        .on_conflict(
            OnConflict::column(tokenizer_vocab::Column::Name)
                .update_columns([
                    tokenizer_vocab::Column::Bytes,
                    tokenizer_vocab::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(conn)
        .await?;
    Ok(())
}
