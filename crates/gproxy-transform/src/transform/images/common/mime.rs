use crate::protocol::openai;

pub(super) fn openai_output_format_mime(format: &openai::ImageOutputFormat) -> &'static str {
    match format {
        openai::ImageOutputFormat::Png => "image/png",
        openai::ImageOutputFormat::Jpeg => "image/jpeg",
        openai::ImageOutputFormat::Webp => "image/webp",
    }
}

pub(super) fn parse_data_url(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix("data:")?;
    let (metadata, data) = value.split_once(',')?;
    let mut parts = metadata.split(';');
    let mime_type = parts.next()?;
    if !parts.any(|part| part.eq_ignore_ascii_case("base64")) || !is_image_mime(mime_type) {
        return None;
    }

    Some((mime_type, data))
}

pub(super) fn infer_mime_type_from_uri(uri: Option<&str>) -> Option<String> {
    let uri = uri?
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if uri.ends_with(".png") {
        Some("image/png".to_owned())
    } else if uri.ends_with(".jpg") || uri.ends_with(".jpeg") {
        Some("image/jpeg".to_owned())
    } else if uri.ends_with(".webp") {
        Some("image/webp".to_owned())
    } else {
        None
    }
}

pub(super) fn is_image_mime(value: &str) -> bool {
    value
        .get(..6)
        .map(|prefix| prefix.eq_ignore_ascii_case("image/"))
        .unwrap_or(false)
}

pub(super) fn is_png_mime(value: &str) -> bool {
    value.eq_ignore_ascii_case("IMAGE_PNG") || value.eq_ignore_ascii_case("image/png")
}

pub(super) fn is_jpeg_mime(value: &str) -> bool {
    value.eq_ignore_ascii_case("IMAGE_JPEG")
        || value.eq_ignore_ascii_case("image/jpeg")
        || value.eq_ignore_ascii_case("image/jpg")
}

pub(super) fn is_webp_mime(value: &str) -> bool {
    value.eq_ignore_ascii_case("IMAGE_WEBP") || value.eq_ignore_ascii_case("image/webp")
}
