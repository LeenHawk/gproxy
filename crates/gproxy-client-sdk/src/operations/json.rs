use super::Params;
use crate::{Backend, Client, OperationKey, Result, wire_json};
use gproxy_protocol::{Operation, openai};
use http::Method;
macro_rules! native_model {
    ($native:ident,required) => {
        Some($native.model.as_str())
    };
    ($native:ident,optional) => {
        $native.model.as_deref()
    };
    ($native:ident,nullable) => {
        $native.model.as_ref().and_then(Option::as_deref)
    };
}

// The operation's type and path are a single declaration. All of these use
// GPROXY's public OpenAI-shaped ingress; the target channel adapts upstream.
macro_rules! json_operation {
    ($feature:literal,$method:ident,$input:ty,$output:ty,$operation:ident,$path:literal,$model:ident) => {
        #[cfg(feature = $feature)]
        impl<B: Backend> Client<B> {
            pub async fn $method(&self, request: &Params<$input>) -> Result<$output> {
                let value = native_params!(request);
                let model = native_model!(value, $model);
                self.json(
                    OperationKey {
                        operation: Operation::$operation,
                        dialect: gproxy_protocol::Dialect::OpenAi,
                    },
                    model,
                    wire_json(Method::POST, $path.into(), &value)?,
                )
                .await
            }
        }
    };
}
json_operation!(
    "embeddings",
    embeddings,
    openai::embeddings::CreateEmbeddingRequestBody,
    openai::embeddings::CreateEmbeddingResponseBody,
    CreateEmbedding,
    "/v1/embeddings",
    required
);
json_operation!(
    "rerank",
    rerank,
    openai::rerank::RerankRequestBody,
    openai::rerank::RerankResponseBody,
    Rerank,
    "/v1/rerank",
    required
);
json_operation!(
    "images",
    create_image,
    openai::images::CreateImageRequestBody,
    openai::images::ImagesResponse,
    CreateImage,
    "/v1/images/generations",
    nullable
);
json_operation!(
    "images",
    edit_image,
    openai::images::EditImageJsonBody,
    openai::images::ImagesResponse,
    EditImage,
    "/v1/images/edits",
    nullable
);
json_operation!(
    "video",
    create_video,
    openai::video::NativeCreateVideoRequestBody,
    openai::video::NativeVideo,
    CreateVideo,
    "/v1/videos",
    optional
);
