use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::value;

pub(crate) fn list() -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("name"))
        .from(Alias::new("tokenizer_vocabs"))
        .order_by(Alias::new("name"), sea_query::Order::Asc);
    Statement::query(&query)
}

pub(crate) fn get(name: &str) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns([Alias::new("bytes")])
        .from(Alias::new("tokenizer_vocabs"))
        .and_where(Expr::col(Alias::new("name")).eq(name))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn put(name: &str, bytes: &[u8], updated_at: i64) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("tokenizer_vocabs"))
        .columns([
            Alias::new("name"),
            Alias::new("bytes"),
            Alias::new("updated_at"),
        ])
        .values_panic([
            value(name.to_owned()),
            value(bytes.to_vec()),
            value(updated_at),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("name"))
                .update_columns([Alias::new("bytes"), Alias::new("updated_at")])
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn delete(name: &str) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new("tokenizer_vocabs"))
        .and_where(Expr::col(Alias::new("name")).eq(name));
    Statement::query(&query)
}
