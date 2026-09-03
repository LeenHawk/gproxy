//! Typed image-generation pairs.

use gproxy_protocol::gemini;
use gproxy_protocol::openai;
use gproxy_protocol::openai::images as image;

use super::RequestContext;

pub mod openai_create_to_gemini_generate_content {
    use super::*;

    pub fn request(
        input: image::CreateImageRequest,
        context: RequestContext<'_>,
    ) -> gemini::GenerateContentRequest {
        crate::images::generate_content::openai_create_request_typed(input, context.upstream_model)
    }

    pub fn response(input: gemini::GenerateContentResponse) -> image::ImagesResponse {
        crate::images::generate_content::gemini_response_to_openai_typed(input)
    }
}

pub mod gemini_generate_content_to_openai_create {
    use super::*;

    pub fn request(
        input: gemini::GenerateContentRequest,
        context: RequestContext<'_>,
    ) -> image::CreateImageRequest {
        crate::images::generate_content::gemini_create_request_typed(input, context.upstream_model)
    }

    pub fn response(input: image::ImagesResponse) -> gemini::GenerateContentResponse {
        crate::images::generate_content::openai_response_to_gemini_typed(input)
    }
}

pub mod openai_edit_to_gemini_generate_content {
    use super::*;

    pub fn request(
        input: image::EditImageRequest,
        context: RequestContext<'_>,
    ) -> gemini::GenerateContentRequest {
        crate::images::generate_content::openai_edit_request_typed(input, context.upstream_model)
    }

    pub fn response(input: gemini::GenerateContentResponse) -> image::ImagesResponse {
        crate::images::generate_content::gemini_response_to_openai_typed(input)
    }
}

pub mod gemini_generate_content_to_openai_edit {
    use super::*;

    pub fn request(
        input: gemini::GenerateContentRequest,
        context: RequestContext<'_>,
    ) -> image::EditImageRequest {
        crate::images::generate_content::gemini_edit_request_typed(input, context.upstream_model)
    }

    pub fn response(input: image::ImagesResponse) -> gemini::GenerateContentResponse {
        crate::images::generate_content::openai_response_to_gemini_typed(input)
    }
}

pub mod openai_create_to_openai_responses {
    use super::*;

    pub fn request(
        input: image::CreateImageRequest,
        context: RequestContext<'_>,
    ) -> openai::ResponseCreateRequest {
        crate::images::responses::create_image_request_typed(input, context.upstream_model)
    }

    pub fn response(input: openai::ResponseObject) -> image::ImagesResponse {
        crate::images::responses::responses_to_images_typed(input)
    }
}

pub mod openai_responses_to_openai_create {
    use super::*;

    pub fn request(
        input: openai::ResponseCreateRequest,
        context: RequestContext<'_>,
    ) -> image::CreateImageRequest {
        crate::images::responses::responses_to_create_request_typed(input, context.upstream_model)
    }

    pub fn response(input: image::ImagesResponse) -> openai::ResponseObject {
        crate::images::responses::images_to_responses_typed(input)
    }
}

pub mod openai_edit_to_openai_responses {
    use super::*;

    pub fn request(
        input: image::EditImageRequest,
        context: RequestContext<'_>,
    ) -> openai::ResponseCreateRequest {
        crate::images::responses::edit_image_request_typed(input, context.upstream_model)
    }

    pub fn response(input: openai::ResponseObject) -> image::ImagesResponse {
        crate::images::responses::responses_to_images_typed(input)
    }
}

pub mod openai_responses_to_openai_edit {
    use super::*;

    pub fn request(
        input: openai::ResponseCreateRequest,
        context: RequestContext<'_>,
    ) -> image::EditImageRequest {
        crate::images::responses::responses_to_edit_request_typed(input, context.upstream_model)
    }

    pub fn response(input: image::ImagesResponse) -> openai::ResponseObject {
        crate::images::responses::images_to_responses_typed(input)
    }
}

pub mod openai_to_gemini_imagen {
    use super::*;

    pub fn request(input: image::CreateImageRequest) -> gemini::ImagenPredictRequest {
        crate::images::imagen::openai_request_typed(input)
    }

    pub fn response(input: gemini::ImagenPredictResponse) -> image::ImagesResponse {
        crate::images::imagen::gemini_response_to_openai_typed(input)
    }
}

pub mod gemini_imagen_to_openai {
    use super::*;

    pub fn request(
        input: gemini::ImagenPredictRequest,
        context: RequestContext<'_>,
    ) -> image::CreateImageRequest {
        crate::images::imagen::gemini_request_typed(input, context.upstream_model)
    }

    pub fn response(input: image::ImagesResponse) -> gemini::ImagenPredictResponse {
        crate::images::imagen::openai_response_to_gemini_typed(input)
    }
}
