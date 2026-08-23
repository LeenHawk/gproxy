mod request;
mod response;
mod stream;
mod types;

pub use request::*;
pub use response::*;
pub use stream::*;
pub use types::*;

pub type ChatCompletionWireModel =
    crate::openai::common::OpenAiWireModel<ChatCompletionRequest, ChatCompletionResponse>;
pub type ChatCompletionStreamWireModel =
    crate::openai::common::OpenAiWireModel<ChatCompletionRequest, ChatCompletionChunk>;
