use sea_query::{Alias, Expr, ExprTrait, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, json, value};
use crate::records::{CaptureInput, RequestLogCompletion, RequestLogImportInput, RequestLogInput};

pub(crate) fn import_request_log(input: &RequestLogImportInput) -> Result<Statement, StoreError> {
    let request = &input.request;
    insert(
        "request_logs",
        &[
            "request_id",
            "at",
            "method",
            "path",
            "query",
            "client_ip",
            "request_headers",
            "request_body",
            "response_status",
            "response_body",
        ],
        vec![
            value(request.request_id.clone()),
            value(request.at),
            value(request.method.clone()),
            value(request.path.clone()),
            value(request.query.clone()),
            value(request.client_ip.clone()),
            value(
                request
                    .request_headers
                    .as_ref()
                    .map(|headers| json(headers, "request headers"))
                    .transpose()?,
            ),
            value(request.request_body.clone()),
            value(input.response_status.map(i64::from)),
            value(input.response_body.clone()),
        ],
    )
}

pub(crate) fn begin_request_log(input: &RequestLogInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("request_logs"))
        .columns([
            Alias::new("request_id"),
            Alias::new("at"),
            Alias::new("method"),
            Alias::new("path"),
            Alias::new("query"),
            Alias::new("client_ip"),
            Alias::new("request_headers"),
            Alias::new("request_body"),
        ])
        .values_panic([
            value(input.request_id.clone()),
            value(input.at),
            value(input.method.clone()),
            value(input.path.clone()),
            value(input.query.clone()),
            value(input.client_ip.clone()),
            value(
                input
                    .request_headers
                    .as_ref()
                    .map(|headers| json(headers, "request headers"))
                    .transpose()?,
            ),
            value(input.request_body.clone()),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("request_id"))
                .do_nothing()
                .to_owned(),
        );
    Statement::query(&query)
}

pub(crate) fn finish_request_log(input: &RequestLogCompletion) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("request_logs"))
        .values([
            (
                Alias::new("response_status"),
                value(i64::from(input.response_status)),
            ),
            (Alias::new("error_kind"), value(input.error_kind.clone())),
            (
                Alias::new("response_headers"),
                value(
                    input
                        .response_headers
                        .as_ref()
                        .map(|headers| json(headers, "response headers"))
                        .transpose()?,
                ),
            ),
            (
                Alias::new("response_body"),
                value(input.response_body.clone()),
            ),
        ])
        .and_where(Expr::col(Alias::new("request_id")).eq(&input.request_id));
    Statement::query(&query)
}

pub(crate) fn update_request_log_response(
    request_id: &str,
    response_body: Vec<u8>,
) -> Result<Statement, StoreError> {
    let mut query = Query::update();
    query
        .table(Alias::new("request_logs"))
        .value(Alias::new("response_body"), value(response_body))
        .and_where(Expr::col(Alias::new("request_id")).eq(request_id));
    Statement::query(&query)
}

pub(crate) fn insert_capture(input: &CaptureInput) -> Result<Statement, StoreError> {
    insert(
        "wire_logs",
        &[
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
        ],
        vec![
            value(input.request_id.clone()),
            value(input.at),
            value(input.provider_id),
            value(input.credential_id),
            value(input.upstream_url.clone()),
            value(input.request_method.clone()),
            value(
                input
                    .request_headers
                    .as_ref()
                    .map(|headers| json(headers, "request headers"))
                    .transpose()?,
            ),
            value(input.response_status.map(i64::from)),
            value(
                input
                    .response_headers
                    .as_ref()
                    .map(|headers| json(headers, "response headers"))
                    .transpose()?,
            ),
            value(input.request_body.clone()),
            value(input.response_body.clone()),
        ],
    )
}
