//! `Client` as the HTTP leg of gproxy-seaorm's libSQL/Turso connection, so
//! the database goes through the same transport as everything else on
//! every target (feature `libsql`).

use crate::{Client, OutboundClient};
use futures_util::StreamExt;
use gproxy_protocol::{HttpBody, connection::Bytes};
use gproxy_seaorm::{LibsqlFuture, LibsqlRequest, LibsqlResponse, LibsqlTransport};
use sea_orm_error::DbErr;

mod sea_orm_error {
    pub use gproxy_seaorm::sea_orm_migration::sea_orm::DbErr;
}

fn db_error(message: impl Into<String>) -> DbErr {
    DbErr::Custom(message.into())
}

impl LibsqlTransport for Client {
    fn post<'a>(
        &'a self,
        request: LibsqlRequest,
    ) -> LibsqlFuture<'a, Result<LibsqlResponse, DbErr>> {
        Box::pin(async move {
            let mut builder = http::Request::builder()
                .method(http::Method::POST)
                .uri(request.url.as_str());
            for (name, value) in &request.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            let outgoing = builder
                .body(HttpBody::Bytes(Bytes::from(request.body)))
                .map_err(|e| db_error(format!("libsql request: {e}")))?;
            let response = self
                .send(outgoing)
                .await
                .map_err(|e| db_error(format!("libsql transport: {e}")))?;
            let body = match response.body {
                HttpBody::Bytes(bytes) => bytes.to_vec(),
                HttpBody::Stream(mut stream) => {
                    let mut out = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.map_err(|e| db_error(format!("libsql body: {e}")))?;
                        out.extend_from_slice(&chunk);
                    }
                    out
                }
            };
            Ok(LibsqlResponse {
                status: response.status.as_u16(),
                body,
            })
        })
    }
}
