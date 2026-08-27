use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::value;
use crate::records::CredentialEnvelope;

pub(crate) fn select_secret_fingerprint(key: &str) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .column(Alias::new("value_json"))
        .from(Alias::new("settings"))
        .and_where(Expr::col(Alias::new("key")).eq(key))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn select_credential_secrets() -> Result<Statement, StoreError> {
    secret_select("credentials", false)
}

pub(crate) fn select_user_key_secrets() -> Result<Statement, StoreError> {
    secret_select("user_keys", true)
}

fn secret_select(table: &'static str, nullable: bool) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "ciphertext",
                "wrapped_key",
                "payload_nonce",
                "key_nonce",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new(table))
        .order_by(Alias::new("id"), sea_query::Order::Asc);
    if nullable {
        query.and_where(Expr::col(Alias::new("ciphertext")).is_not_null());
    }
    Statement::query(&query)
}

pub(crate) fn update_secret(
    table: &'static str,
    id: i64,
    envelope: &CredentialEnvelope,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query.table(Alias::new(table)).values([
        (Alias::new("ciphertext"), value(envelope.ciphertext.clone())),
        (
            Alias::new("wrapped_key"),
            value(envelope.wrapped_key.clone()),
        ),
        (
            Alias::new("payload_nonce"),
            value(envelope.payload_nonce.clone()),
        ),
        (Alias::new("key_nonce"), value(envelope.key_nonce.clone())),
    ]);
    if table == "credentials" {
        query.value(
            Alias::new("version"),
            Expr::col(Alias::new("version")).add(1),
        );
    }
    query.and_where(Expr::col(Alias::new("id")).eq(id));
    Statement::query(&query)
}
