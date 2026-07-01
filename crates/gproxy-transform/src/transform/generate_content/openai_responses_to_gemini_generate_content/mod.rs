//! OpenAI Responses -> Gemini GenerateContent transforms.

mod request;
mod response;
mod stream;

pub use request::request;
pub use response::response;
pub use stream::{StreamTransform, stream_event};
