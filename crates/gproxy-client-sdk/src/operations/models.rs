use super::empty_request;
use crate::*;
use gproxy_protocol::{Operation, openai};
use http::Method;
#[cfg(feature = "models")]
impl<B: Backend> Client<B> {
    /// Returns the normal permission-filtered local catalog. Its owner field
    /// is not a dialect contract; callers explicitly construct `Model`.
    pub async fn list_models(&self) -> Result<openai::models::ListModelsResponseBody> {
        self.json(
            OperationKey {
                operation: Operation::ListModels,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::GET, "/v1/models".into()),
        )
        .await
    }
}
