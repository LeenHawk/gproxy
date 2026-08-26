use sea_query::{Alias, Expr, ExprTrait, JoinType, Order, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::records::LogQuery;

const REQUEST_COLUMNS: &[&str] = &[
    "id",
    "request_id",
    "at",
    "method",
    "path",
    "query",
    "request_headers",
    "request_body",
    "response_status",
    "error_kind",
    "response_headers",
    "response_body",
];
const WIRE_COLUMNS: &[&str] = &[
    "id",
    "request_id",
    "at",
    "provider_id",
    "credential_id",
    "upstream_url",
    "request_method",
    "request_headers",
    "response_status",
    "response_headers",
    "request_body",
    "response_body",
];

pub(crate) fn list_logs(input: &LogQuery) -> Result<Statement, StoreError> {
    let requests = Alias::new("r");
    let mut query = Query::select();
    query
        .distinct()
        .columns(
            [
                "id",
                "request_id",
                "at",
                "method",
                "path",
                "response_status",
                "error_kind",
            ]
            .map(|column| (requests.clone(), Alias::new(column))),
        )
        .from_as(Alias::new("request_logs"), requests.clone())
        .and_where(Expr::col((requests.clone(), Alias::new("at"))).gte(input.start))
        .and_where(Expr::col((requests.clone(), Alias::new("at"))).lt(input.end))
        .order_by((requests.clone(), Alias::new("at")), Order::Desc)
        .order_by((requests.clone(), Alias::new("id")), Order::Desc)
        .limit(input.limit.saturating_add(1));
    if let Some(cursor) = input.cursor {
        query.and_where(Expr::col((requests.clone(), Alias::new("id"))).lt(cursor));
    }
    if let Some(status) = input.status {
        query.and_where(
            Expr::col((requests.clone(), Alias::new("response_status"))).eq(i64::from(status)),
        );
    }
    if let Some(request_id) = &input.request_id {
        query.and_where(
            Expr::col((requests.clone(), Alias::new("request_id"))).eq(request_id.clone()),
        );
    }
    if let Some(provider_id) = input.provider_id {
        query.join(
            JoinType::InnerJoin,
            Alias::new("wire_logs"),
            Expr::col((Alias::new("wire_logs"), Alias::new("request_id")))
                .equals((requests.clone(), Alias::new("request_id"))),
        );
        query.and_where(
            Expr::col((Alias::new("wire_logs"), Alias::new("provider_id"))).eq(provider_id),
        );
    }
    add_identity_filters(&mut query, &requests, input);
    Statement::query(&query)
}

fn add_identity_filters(
    query: &mut sea_query::SelectStatement,
    requests: &Alias,
    input: &LogQuery,
) {
    if input.user_id.is_none() && input.user_key_id.is_none() {
        return;
    }
    query.join(
        JoinType::InnerJoin,
        Alias::new("usage_rows"),
        Expr::col((Alias::new("usage_rows"), Alias::new("request_id")))
            .equals((requests.clone(), Alias::new("request_id"))),
    );
    for (column, filter) in [
        ("user_id", input.user_id),
        ("user_key_id", input.user_key_id),
    ] {
        if let Some(value) = filter {
            query.and_where(Expr::col((Alias::new("usage_rows"), Alias::new(column))).eq(value));
        }
    }
}

pub(crate) fn request_log(request_id: &str) -> Result<Statement, StoreError> {
    select_exchange("request_logs", REQUEST_COLUMNS, request_id, false)
}

pub(crate) fn wire_logs(request_id: &str) -> Result<Statement, StoreError> {
    select_exchange("wire_logs", WIRE_COLUMNS, request_id, true)
}

fn select_exchange(
    table: &'static str,
    columns: &[&'static str],
    request_id: &str,
    ordered: bool,
) -> Result<Statement, StoreError> {
    let mut query = Query::select();
    query
        .columns(columns.iter().copied().map(Alias::new))
        .from(Alias::new(table))
        .and_where(Expr::col(Alias::new("request_id")).eq(request_id));
    if ordered {
        query
            .order_by(Alias::new("at"), Order::Asc)
            .order_by(Alias::new("id"), Order::Asc);
    }
    Statement::query(&query)
}
