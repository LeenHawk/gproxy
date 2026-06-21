//! Tokenizer vocab ops for the libSQL edge backend: raw `tokenizer.json` BLOBs
//! in the `tokenizer_vocabs` table, upsert-on-put.

use crate::store::libsql::{LibsqlClient, arg_blob, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{col_blob, col_str};
use crate::store::persistence::libsql::util::{exec, now_secs, query, query_one};

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<String>> {
    query(client, "SELECT name FROM tokenizer_vocabs", &[])
        .await?
        .iter()
        .map(|r| col_str(r, 0))
        .collect()
}

pub async fn get(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    query_one(
        client,
        "SELECT bytes FROM tokenizer_vocabs WHERE name = ?",
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(|r| col_blob(r, 0))
    .transpose()
}

pub async fn put(client: &LibsqlClient, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    exec(
        client,
        "INSERT INTO tokenizer_vocabs (name, bytes, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(name) DO UPDATE SET bytes = excluded.bytes, updated_at = excluded.updated_at",
        &[arg_text(name), arg_blob(bytes), arg_integer(now_secs())],
    )
    .await?;
    Ok(())
}
