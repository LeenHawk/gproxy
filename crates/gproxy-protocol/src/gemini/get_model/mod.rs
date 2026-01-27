pub mod types;
pub mod request;
pub mod response;

pub use request::{GetModelPath, GetModelRequest};
pub use response::GetModelResponse;
pub use types::Model;
