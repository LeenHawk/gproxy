use crate::protocol::{gemini, openai};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::transform::images) struct ImageShape {
    pub aspect_ratio: Option<gemini::ImageAspectRatio>,
    pub image_size: Option<gemini::ImageSize>,
}

pub(in crate::transform::images) fn create_size_to_shape(
    size: Option<openai::ImageSize>,
) -> Option<ImageShape> {
    match size? {
        openai::ImageSize::Known(openai::ImageSizeKnown::Auto) => None,
        openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1024) => Some(shape(
            gemini::ImageAspectRatioKnown::OneToOne,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageSize::Known(openai::ImageSizeKnown::Size1536By1024) => Some(shape(
            gemini::ImageAspectRatioKnown::ThreeToTwo,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1536) => Some(shape(
            gemini::ImageAspectRatioKnown::TwoToThree,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageSize::Known(openai::ImageSizeKnown::Size512By512) => Some(shape(
            gemini::ImageAspectRatioKnown::OneToOne,
            Some(gemini::ImageSizeKnown::Size512),
        )),
        openai::ImageSize::Known(openai::ImageSizeKnown::Size256By256) => {
            Some(shape(gemini::ImageAspectRatioKnown::OneToOne, None))
        }
        openai::ImageSize::Known(openai::ImageSizeKnown::Size1792By1024) => Some(shape(
            gemini::ImageAspectRatioKnown::SixteenToNine,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1792) => Some(shape(
            gemini::ImageAspectRatioKnown::NineToSixteen,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageSize::Unknown(value) => dimensions_to_shape(&value),
    }
}

pub(in crate::transform::images) fn edit_size_to_shape(
    size: Option<openai::ImageEditSize>,
) -> Option<ImageShape> {
    match size? {
        openai::ImageEditSize::Auto => None,
        openai::ImageEditSize::Size1024By1024 => Some(shape(
            gemini::ImageAspectRatioKnown::OneToOne,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageEditSize::Size1536By1024 => Some(shape(
            gemini::ImageAspectRatioKnown::ThreeToTwo,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
        openai::ImageEditSize::Size1024By1536 => Some(shape(
            gemini::ImageAspectRatioKnown::TwoToThree,
            Some(gemini::ImageSizeKnown::Size1K),
        )),
    }
}

fn shape(
    aspect_ratio: gemini::ImageAspectRatioKnown,
    image_size: Option<gemini::ImageSizeKnown>,
) -> ImageShape {
    ImageShape {
        aspect_ratio: Some(gemini::ImageAspectRatio::Known(aspect_ratio)),
        image_size: image_size.map(gemini::ImageSize::Known),
    }
}

fn dimensions_to_shape(value: &str) -> Option<ImageShape> {
    let (width, height) = parse_dimensions(value)?;
    let aspect_ratio = dimensions_to_aspect_ratio(width, height);
    let image_size = max_dimension_to_image_size(width.max(height));

    if aspect_ratio.is_none() && image_size.is_none() {
        return None;
    }

    Some(ImageShape {
        aspect_ratio,
        image_size,
    })
}

fn parse_dimensions(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

fn dimensions_to_aspect_ratio(width: u32, height: u32) -> Option<gemini::ImageAspectRatio> {
    let divisor = gcd(width, height);
    let ratio = (width / divisor, height / divisor);
    let known = match ratio {
        (1, 1) => gemini::ImageAspectRatioKnown::OneToOne,
        (1, 4) => gemini::ImageAspectRatioKnown::OneToFour,
        (4, 1) => gemini::ImageAspectRatioKnown::FourToOne,
        (1, 8) => gemini::ImageAspectRatioKnown::OneToEight,
        (8, 1) => gemini::ImageAspectRatioKnown::EightToOne,
        (2, 3) => gemini::ImageAspectRatioKnown::TwoToThree,
        (3, 2) => gemini::ImageAspectRatioKnown::ThreeToTwo,
        (3, 4) => gemini::ImageAspectRatioKnown::ThreeToFour,
        (4, 3) => gemini::ImageAspectRatioKnown::FourToThree,
        (4, 5) => gemini::ImageAspectRatioKnown::FourToFive,
        (5, 4) => gemini::ImageAspectRatioKnown::FiveToFour,
        (9, 16) => gemini::ImageAspectRatioKnown::NineToSixteen,
        (16, 9) => gemini::ImageAspectRatioKnown::SixteenToNine,
        (21, 9) | (7, 3) => gemini::ImageAspectRatioKnown::TwentyOneToNine,
        _ => return None,
    };
    Some(gemini::ImageAspectRatio::Known(known))
}

fn max_dimension_to_image_size(max_dimension: u32) -> Option<gemini::ImageSize> {
    let known = if max_dimension <= 512 {
        gemini::ImageSizeKnown::Size512
    } else if max_dimension <= 1024 {
        gemini::ImageSizeKnown::Size1K
    } else if max_dimension <= 2048 {
        gemini::ImageSizeKnown::Size2K
    } else if max_dimension <= 4096 {
        gemini::ImageSizeKnown::Size4K
    } else {
        return None;
    };
    Some(gemini::ImageSize::Known(known))
}

pub(in crate::transform::images) fn gemini_to_openai_create_size(
    config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ImageSize> {
    let shape = gemini_image_shape(config)?;
    openai_create_size(shape.aspect_ratio.as_ref(), shape.image_size.as_ref())
}

pub(in crate::transform::images) fn gemini_to_openai_edit_size(
    config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ImageEditSize> {
    let shape = gemini_image_shape(config)?;
    openai_edit_size(shape.aspect_ratio.as_ref(), shape.image_size.as_ref())
}

fn gemini_image_shape(config: Option<&gemini::GenerationConfig>) -> Option<&gemini::ImageConfig> {
    config?.image_config.as_ref()
}

fn openai_create_size(
    aspect_ratio: Option<&gemini::ImageAspectRatio>,
    image_size: Option<&gemini::ImageSize>,
) -> Option<openai::ImageSize> {
    if matches!(image_size, Some(size) if should_format_non_standard_size(aspect_ratio, size)) {
        return Some(openai::ImageSize::Unknown(format_dimensions(
            aspect_ratio?,
            image_size?,
        )?));
    }

    let size = match (aspect_ratio, image_size) {
        (
            Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::OneToOne)),
            Some(gemini::ImageSize::Known(gemini::ImageSizeKnown::Size512)),
        ) => openai::ImageSize::Known(openai::ImageSizeKnown::Size512By512),
        (Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::OneToOne)), _) => {
            openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1024)
        }
        (Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::ThreeToTwo)), _) => {
            openai::ImageSize::Known(openai::ImageSizeKnown::Size1536By1024)
        }
        (Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::TwoToThree)), _) => {
            openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1536)
        }
        (
            Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::SixteenToNine)),
            _,
        ) => openai::ImageSize::Known(openai::ImageSizeKnown::Size1792By1024),
        (
            Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::NineToSixteen)),
            _,
        ) => openai::ImageSize::Known(openai::ImageSizeKnown::Size1024By1792),
        (Some(aspect_ratio), Some(image_size)) => {
            openai::ImageSize::Unknown(format_dimensions(aspect_ratio, image_size)?)
        }
        _ => return None,
    };
    Some(size)
}

fn should_format_non_standard_size(
    aspect_ratio: Option<&gemini::ImageAspectRatio>,
    image_size: &gemini::ImageSize,
) -> bool {
    match (aspect_ratio, image_size) {
        (
            Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::OneToOne)),
            gemini::ImageSize::Known(gemini::ImageSizeKnown::Size512),
        )
        | (_, gemini::ImageSize::Known(gemini::ImageSizeKnown::Size1K)) => false,
        (_, gemini::ImageSize::Known(gemini::ImageSizeKnown::Size512))
        | (_, gemini::ImageSize::Known(gemini::ImageSizeKnown::Size2K))
        | (_, gemini::ImageSize::Known(gemini::ImageSizeKnown::Size4K))
        | (_, gemini::ImageSize::Unknown(_)) => true,
    }
}

fn openai_edit_size(
    aspect_ratio: Option<&gemini::ImageAspectRatio>,
    _: Option<&gemini::ImageSize>,
) -> Option<openai::ImageEditSize> {
    match aspect_ratio {
        Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::OneToOne)) => {
            Some(openai::ImageEditSize::Size1024By1024)
        }
        Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::ThreeToTwo)) => {
            Some(openai::ImageEditSize::Size1536By1024)
        }
        Some(gemini::ImageAspectRatio::Known(gemini::ImageAspectRatioKnown::TwoToThree)) => {
            Some(openai::ImageEditSize::Size1024By1536)
        }
        _ => None,
    }
}

fn format_dimensions(
    aspect_ratio: &gemini::ImageAspectRatio,
    image_size: &gemini::ImageSize,
) -> Option<String> {
    let (width_ratio, height_ratio) = aspect_ratio_components(aspect_ratio)?;
    let max_dimension = image_size_dimension(image_size)?;

    if width_ratio >= height_ratio {
        let height = max_dimension * height_ratio / width_ratio;
        Some(format!("{max_dimension}x{height}"))
    } else {
        let width = max_dimension * width_ratio / height_ratio;
        Some(format!("{width}x{max_dimension}"))
    }
}

fn aspect_ratio_components(aspect_ratio: &gemini::ImageAspectRatio) -> Option<(u32, u32)> {
    let known = match aspect_ratio {
        gemini::ImageAspectRatio::Known(known) => known,
        gemini::ImageAspectRatio::Unknown(value) => {
            let (width, height) = value.split_once(':')?;
            return Some((width.parse().ok()?, height.parse().ok()?));
        }
    };

    Some(match known {
        gemini::ImageAspectRatioKnown::OneToOne => (1, 1),
        gemini::ImageAspectRatioKnown::OneToFour => (1, 4),
        gemini::ImageAspectRatioKnown::FourToOne => (4, 1),
        gemini::ImageAspectRatioKnown::OneToEight => (1, 8),
        gemini::ImageAspectRatioKnown::EightToOne => (8, 1),
        gemini::ImageAspectRatioKnown::TwoToThree => (2, 3),
        gemini::ImageAspectRatioKnown::ThreeToTwo => (3, 2),
        gemini::ImageAspectRatioKnown::ThreeToFour => (3, 4),
        gemini::ImageAspectRatioKnown::FourToThree => (4, 3),
        gemini::ImageAspectRatioKnown::FourToFive => (4, 5),
        gemini::ImageAspectRatioKnown::FiveToFour => (5, 4),
        gemini::ImageAspectRatioKnown::NineToSixteen => (9, 16),
        gemini::ImageAspectRatioKnown::SixteenToNine => (16, 9),
        gemini::ImageAspectRatioKnown::TwentyOneToNine => (21, 9),
    })
}

fn image_size_dimension(image_size: &gemini::ImageSize) -> Option<u32> {
    match image_size {
        gemini::ImageSize::Known(gemini::ImageSizeKnown::Size512) => Some(512),
        gemini::ImageSize::Known(gemini::ImageSizeKnown::Size1K) => Some(1024),
        gemini::ImageSize::Known(gemini::ImageSizeKnown::Size2K) => Some(2048),
        gemini::ImageSize::Known(gemini::ImageSizeKnown::Size4K) => Some(4096),
        gemini::ImageSize::Unknown(value) => {
            let value = value.strip_suffix('K')?;
            Some(value.parse::<u32>().ok()? * 1024)
        }
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a.max(1)
}
