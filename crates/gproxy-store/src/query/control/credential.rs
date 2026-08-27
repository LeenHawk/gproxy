use sea_query::{Alias, Expr, ExprTrait, JoinType, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{unsigned, value};
use crate::records::CredentialEnvelope;

pub(crate) fn load_credential(id: i64) -> Result<Statement, StoreError> {
    let credential = Alias::new("credential");
    let provider = Alias::new("provider");
    let mut query = Query::select();
    for column in [
        "id",
        "provider_id",
        "label",
        "kind",
        "ciphertext",
        "wrapped_key",
        "payload_nonce",
        "key_nonce",
        "version",
        "enabled",
    ] {
        query.expr_as(
            Expr::col((credential.clone(), Alias::new(column))),
            Alias::new(column),
        );
    }
    query
        .expr_as(
            Expr::col((provider.clone(), Alias::new("channel"))),
            Alias::new("channel"),
        )
        .from_as(Alias::new("credentials"), credential.clone())
        .join_as(
            JoinType::InnerJoin,
            Alias::new("providers"),
            provider.clone(),
            Expr::col((provider, Alias::new("id")))
                .eq(Expr::col((credential.clone(), Alias::new("provider_id")))),
        )
        .and_where(Expr::col((credential, Alias::new("id"))).eq(id))
        .limit(1);
    Statement::query(&query)
}

pub(crate) fn compare_and_swap_credential(
    id: i64,
    envelope: &CredentialEnvelope,
    version: u64,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("credentials"))
        .values([
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
            (
                Alias::new("version"),
                Expr::col(Alias::new("version")).add(1),
            ),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .and_where(Expr::col(Alias::new("enabled")).eq(true))
        .and_where(Expr::col(Alias::new("version")).eq(unsigned(version, "credential version")?));
    Statement::query(&query)
}
