use super::request::enum_string;
use crate::{transform::TransformError, wire::openai::video as o};

pub(super) fn dimensions(
    input: &o::CreateVideoRequestBody,
) -> Result<(Option<String>, Option<String>), TransformError> {
    let explicit_aspect = input.aspect_ratio.map(enum_string);
    let explicit_resolution = input.resolution.map(enum_string);
    if let Some(size) = &input.size {
        let inferred = parse_size(size).ok_or_else(|| {
            TransformError::unsupported("size", "size is not a supported WxH Veo mapping")
        })?;
        if explicit_aspect.is_some() && explicit_aspect != inferred.0 {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "size/aspect_ratio",
                "size and aspect ratio disagree",
            ));
        }
        if explicit_resolution.is_some() && explicit_resolution != inferred.1 {
            return Err(TransformError::new(
                crate::transform::TransformErrorKind::Conflict,
                "size/resolution",
                "size and resolution disagree",
            ));
        }
        return Ok((
            explicit_aspect.or(inferred.0),
            explicit_resolution.or(inferred.1),
        ));
    }

    Ok((explicit_aspect, explicit_resolution))
}

fn parse_size(value: &str) -> Option<(Option<String>, Option<String>)> {
    let (width, height) = value.split_once('x')?;
    let width: u32 = width.parse().ok()?;
    let height: u32 = height.parse().ok()?;
    let aspect = match (width, height) {
        (1280, 720) | (1920, 1080) | (3840, 2160) => "16:9",
        (720, 1280) | (1080, 1920) | (2160, 3840) => "9:16",
        _ => return None,
    };
    let resolution = if width.max(height) == 3840 {
        "4K"
    } else if width.max(height) >= 1920 {
        "1080p"
    } else {
        "720p"
    };
    Some((Some(aspect.into()), Some(resolution.into())))
}
