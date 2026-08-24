mod request;
mod response;
mod stream;

pub use request::*;
pub use response::*;
pub use stream::*;

pub type ImageGenerationStreamWireModel =
    super::common::OpenAiWireModel<CreateImageRequest, ImageGenerationStreamEvent>;
pub type ImageEditStreamWireModel =
    super::common::OpenAiWireModel<EditImageRequest, ImageEditStreamEvent>;
