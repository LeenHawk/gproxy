//! Create/Edit Images to a Responses image-generation tool or Gemini.
//!
//! Builders produce one native call; `ImageRequestContext::n` tells the adapter
//! how many independent calls are required. The host chooses the generation
//! model separately from the Responses image tool model. Explicit equivalent
//! controls are mapped; unsupported controls fail before any resource read.
//! In particular Gemini Developer API does not accept `responseMimeType` for
//! images, and its resolution classes cannot guarantee exact Images pixel sizes.
//!
//! Output MIME and dimensions come from validated PNG/JPEG/WebP containers,
//! never from requested settings. This is container validation, not pixel
//! decoding or transcoding. Native usage remains typed per call because generic
//! generation counters cannot fabricate Images' image/text token breakdown.

mod request;
mod response;
pub use request::{
    ImageDialect, ImageDialectBody, ImageInput, ImageRequestContext, ImageResponseFacts,
    ImageTargetModels, PreparedImageRequest, ResolvedImageInput, build_request, image_reference,
    preflight,
};
pub use response::{
    GeneratedImagePart, ImageCallResult, ImageMetadata, ImageUsageFacts, decode_image,
    image_response_from_gemini, image_response_from_responses, inspect_image,
};
