use crate::protocol::{gemini, openai};

use super::mime::{is_jpeg_mime, is_png_mime, is_webp_mime};
use super::scalar::u32_to_i32;
use super::size::ImageShape;

pub(in crate::transform::images) fn generation_config(
    n: Option<u32>,
    size: Option<ImageShape>,
    output_format: Option<openai::ImageOutputFormat>,
    response_format: Option<openai::ImageResponseFormat>,
) -> gemini::GenerationConfig {
    gemini::GenerationConfig {
        response_modalities: vec![gemini::ResponseModality::Known(
            gemini::ResponseModalityKnown::Image,
        )],
        candidate_count: n.map(u32_to_i32),
        image_config: size.and_then(image_config),
        response_format: response_format_config(output_format, response_format),
        ..Default::default()
    }
}

fn image_config(shape: ImageShape) -> Option<gemini::ImageConfig> {
    if shape.aspect_ratio.is_none() && shape.image_size.is_none() {
        return None;
    }

    Some(gemini::ImageConfig {
        aspect_ratio: shape.aspect_ratio,
        image_size: shape.image_size,
        extra: Default::default(),
    })
}

fn response_format_config(
    output_format: Option<openai::ImageOutputFormat>,
    response_format: Option<openai::ImageResponseFormat>,
) -> Option<gemini::ResponseFormatConfig> {
    let image = gemini::ImageResponseFormat {
        mime_type: output_format.and_then(openai_output_format_to_gemini),
        delivery: response_format.map(openai_response_format_to_gemini),
        aspect_ratio: None,
        image_size: None,
        extra: Default::default(),
    };

    if image == gemini::ImageResponseFormat::default() {
        return None;
    }

    Some(gemini::ResponseFormatConfig {
        image: Some(image),
        ..Default::default()
    })
}

fn openai_output_format_to_gemini(
    format: openai::ImageOutputFormat,
) -> Option<gemini::ImageResponseFormatMimeType> {
    match format {
        openai::ImageOutputFormat::Jpeg => Some(gemini::ImageResponseFormatMimeType::Known(
            gemini::ImageResponseFormatMimeTypeKnown::ImageJpeg,
        )),
        openai::ImageOutputFormat::Png | openai::ImageOutputFormat::Webp => None,
    }
}

fn openai_response_format_to_gemini(
    format: openai::ImageResponseFormat,
) -> gemini::ImageResponseDelivery {
    let delivery = match format {
        openai::ImageResponseFormat::B64Json => gemini::ImageResponseDeliveryKnown::Inline,
        openai::ImageResponseFormat::Url => gemini::ImageResponseDeliveryKnown::Uri,
    };
    gemini::ImageResponseDelivery::Known(delivery)
}

pub(in crate::transform::images) fn gemini_response_format(
    config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ImageResponseFormat> {
    let delivery = config
        .and_then(|config| config.response_format.as_ref())
        .and_then(|format| format.image.as_ref())
        .and_then(|image| image.delivery.as_ref())?;

    match delivery {
        gemini::ImageResponseDelivery::Known(gemini::ImageResponseDeliveryKnown::Inline) => {
            Some(openai::ImageResponseFormat::B64Json)
        }
        gemini::ImageResponseDelivery::Known(gemini::ImageResponseDeliveryKnown::Uri) => {
            Some(openai::ImageResponseFormat::Url)
        }
        gemini::ImageResponseDelivery::Known(
            gemini::ImageResponseDeliveryKnown::DeliveryUnspecified,
        ) => None,
        gemini::ImageResponseDelivery::Unknown(value) if value == "INLINE" => {
            Some(openai::ImageResponseFormat::B64Json)
        }
        gemini::ImageResponseDelivery::Unknown(value) if value == "URI" => {
            Some(openai::ImageResponseFormat::Url)
        }
        gemini::ImageResponseDelivery::Unknown(_) => None,
    }
}

pub(in crate::transform::images) fn gemini_output_format(
    config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ImageOutputFormat> {
    let mime_type = config
        .and_then(|config| config.response_format.as_ref())
        .and_then(|format| format.image.as_ref())
        .and_then(|image| image.mime_type.as_ref())?;

    match mime_type {
        gemini::ImageResponseFormatMimeType::Known(
            gemini::ImageResponseFormatMimeTypeKnown::ImageJpeg,
        ) => Some(openai::ImageOutputFormat::Jpeg),
        gemini::ImageResponseFormatMimeType::Known(
            gemini::ImageResponseFormatMimeTypeKnown::MimeTypeUnspecified,
        ) => None,
        gemini::ImageResponseFormatMimeType::Unknown(value) if is_png_mime(value) => {
            Some(openai::ImageOutputFormat::Png)
        }
        gemini::ImageResponseFormatMimeType::Unknown(value) if is_webp_mime(value) => {
            Some(openai::ImageOutputFormat::Webp)
        }
        gemini::ImageResponseFormatMimeType::Unknown(value) if is_jpeg_mime(value) => {
            Some(openai::ImageOutputFormat::Jpeg)
        }
        gemini::ImageResponseFormatMimeType::Unknown(_) => None,
    }
}
