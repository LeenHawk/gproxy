use sea_query::{Alias, Expr, ExprTrait, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, json, value};
use crate::records::AuditEventInput;

pub(crate) fn insert_audit_event(input: &AuditEventInput) -> Result<Statement, StoreError> {
    insert(
        "admin_audit_events",
        &[
            "actor_user_id",
            "action",
            "target_kind",
            "target_id",
            "at",
            "details_json",
        ],
        vec![
            value(input.actor_user_id),
            value(input.action.clone()),
            value(input.target_kind.clone()),
            value(input.target_id),
            value(input.at),
            value(
                input
                    .details
                    .as_ref()
                    .map(|details| json(details, "audit details"))
                    .transpose()?,
            ),
        ],
    )
}

pub(crate) fn select_audit_events(limit: u64) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(
            [
                "id",
                "actor_user_id",
                "action",
                "target_kind",
                "target_id",
                "at",
                "details_json",
            ]
            .into_iter()
            .map(Alias::new),
        )
        .from(Alias::new("admin_audit_events"))
        .order_by(Alias::new("id"), Order::Desc)
        .limit(limit);
    Statement::query(&query)
}

pub(crate) fn select_user_key_secret(id: i64) -> Result<Statement, StoreError> {
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
        .from(Alias::new("user_keys"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .limit(1);
    Statement::query(&query)
}
