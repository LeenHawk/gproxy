mod requests;
mod responses;
mod stream;

pub use requests::*;
pub use responses::*;
pub use stream::*;

use super::common::OpenAiWireModel;

pub type ImageGenerationWireModel = OpenAiWireModel<ImageGenerationRequest, ImagesResponse>;
pub type ImageGenerationStreamWireModel =
    OpenAiWireModel<ImageGenerationRequest, ImageGenerationStreamEvent>;
pub type ImageEditWireModel = OpenAiWireModel<ImageEditRequest, ImagesResponse>;
pub type ImageEditStreamWireModel = OpenAiWireModel<ImageEditRequest, ImageEditStreamEvent>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_edit_accepts_generic_multipart_json_shape() {
        let req: ImageEditRequest = serde_json::from_str(
            r#"{
                "image": [
                    "data:image/png;base64,AAAA",
                    "file_123"
                ],
                "mask": "data:image/png;base64,BBBB",
                "prompt": "make it blue",
                "model": "gpt-image-1.5",
                "n": "2",
                "stream": "true"
            }"#,
        )
        .unwrap();

        assert_eq!(req.images.len(), 2);
        assert_eq!(
            req.images[0].image_url.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(req.images[1].file_id.as_deref(), Some("file_123"));
        assert_eq!(
            req.mask.as_ref().and_then(|mask| mask.image_url.as_deref()),
            Some("data:image/png;base64,BBBB")
        );
        assert_eq!(req.n, Some(2));
        assert_eq!(req.stream, Some(true));
    }
}
