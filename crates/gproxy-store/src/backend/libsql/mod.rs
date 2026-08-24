mod wire;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

use super::{DbFuture, Executor, QueryResult, Statement};
use crate::StoreError;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type HttpFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<Vec<u8>, StoreError>> + Send + 'a>>;
#[cfg(target_arch = "wasm32")]
pub(crate) type HttpFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<Vec<u8>, StoreError>> + 'a>>;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) trait HttpSender: Send + Sync {
    fn post<'a>(&'a self, url: &'a str, auth_token: &'a str, body: Vec<u8>) -> HttpFuture<'a>;
}

#[cfg(target_arch = "wasm32")]
pub(crate) trait HttpSender {
    fn post<'a>(&'a self, url: &'a str, auth_token: &'a str, body: Vec<u8>) -> HttpFuture<'a>;
}

pub(super) struct LibsqlHttp {
    pipeline_url: String,
    auth_token: String,
    sender: Box<dyn HttpSender>,
}

impl LibsqlHttp {
    pub(super) fn new(url: String, auth_token: String) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let sender = native::NativeSender::new();
        #[cfg(target_arch = "wasm32")]
        let sender = wasm::WasmSender;
        Self::with_boxed_sender(url, auth_token, Box::new(sender))
    }

    #[cfg(test)]
    pub(crate) fn with_sender(
        url: String,
        auth_token: String,
        sender: impl HttpSender + 'static,
    ) -> Self {
        Self::with_boxed_sender(url, auth_token, Box::new(sender))
    }

    fn with_boxed_sender(url: String, auth_token: String, sender: Box<dyn HttpSender>) -> Self {
        Self {
            pipeline_url: format!("{}/v2/pipeline", url.trim_end_matches('/')),
            auth_token,
            sender,
        }
    }

    async fn send(&self, request: Vec<u8>) -> Result<Vec<u8>, StoreError> {
        self.sender
            .post(&self.pipeline_url, &self.auth_token, request)
            .await
    }

    async fn execute_one(&self, statement: Statement) -> Result<QueryResult, StoreError> {
        let response = self.send(wire::encode_execute(statement)?).await?;
        wire::decode_execute(&response)
    }

    async fn execute_batch(
        &self,
        statements: Vec<Statement>,
    ) -> Result<Vec<QueryResult>, StoreError> {
        let count = statements.len();
        let response = self.send(wire::encode_batch(statements)?).await?;
        wire::decode_batch(&response, count)
    }
}

impl Executor for LibsqlHttp {
    fn execute<'a>(&'a self, statement: Statement) -> DbFuture<'a, QueryResult> {
        Box::pin(async move { self.execute_one(statement).await })
    }

    fn batch<'a>(&'a self, statements: Vec<Statement>) -> DbFuture<'a, Vec<QueryResult>> {
        Box::pin(async move { self.execute_batch(statements).await })
    }
}

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::InvalidData {
        field: "libsql_response",
        message: message.into(),
    }
}
