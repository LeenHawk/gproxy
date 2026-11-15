//! Makes an image uniform before it is stored or sent: upright, within a
//! size every provider accepts, in a format every provider reads, and
//! without the metadata a camera or editor attached.
use std::io::Cursor;

use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, codecs::jpeg::JpegEncoder,
    imageops::FilterType,
};

/// What a normalized image is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImagePolicy {
    /// The longest edge allowed, in pixels. Larger images are scaled down.
    pub max_edge: u32,
    /// The longest edge of the thumbnail made alongside, if one is wanted.
    /// An image that already fits within it gets none.
    pub thumbnail_edge: Option<u32>,
    /// 1 to 100, for images stored as JPEG.
    pub jpeg_quality: u8,
}

impl Default for ImagePolicy {
    /// 2048 pixels is within what Claude, OpenAI and Gemini take without
    /// scaling the image down themselves.
    fn default() -> Self {
        Self {
            max_edge: 2048,
            thumbnail_edge: Some(512),
            jpeg_quality: 85,
        }
    }
}

/// An image after normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedImage {
    pub bytes: Vec<u8>,
    /// `image/png` if the image has transparency, `image/jpeg` otherwise.
    pub media_type: &'static str,
    pub width: u32,
    pub height: u32,
    /// In the same format as the image.
    pub thumbnail: Option<Vec<u8>>,
}

/// Normalizes an encoded image. `None` means the bytes are to be kept as
/// they are: they are no image this can decode, or they are a GIF, which
/// would lose its animation.
pub fn normalize_image(bytes: &[u8], policy: &ImagePolicy) -> Option<NormalizedImage> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if reader.format()? == ImageFormat::Gif {
        return None;
    }
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut image = DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    if image.width().max(image.height()) > policy.max_edge {
        image = image.resize(policy.max_edge, policy.max_edge, FilterType::Lanczos3);
    }

    let transparent = image.color().has_alpha();
    let encode = |image: &DynamicImage| {
        let mut bytes = Vec::new();
        if transparent {
            let written = image.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png);
            written.expect("a decoded image encodes as PNG");
        } else {
            let encoder = JpegEncoder::new_with_quality(&mut bytes, policy.jpeg_quality);
            let written = image.to_rgb8().write_with_encoder(encoder);
            written.expect("a decoded image encodes as JPEG");
        }
        bytes
    };
    Some(NormalizedImage {
        thumbnail: policy
            .thumbnail_edge
            .filter(|edge| image.width().max(image.height()) > *edge)
            .map(|edge| encode(&image.thumbnail(edge, edge))),
        bytes: encode(&image),
        media_type: if transparent {
            "image/png"
        } else {
            "image/jpeg"
        },
        width: image.width(),
        height: image.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    fn encoded(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), format)
            .unwrap();
        bytes
    }

    fn size(bytes: &[u8]) -> (u32, u32) {
        let image = image::load_from_memory(bytes).unwrap();
        (image.width(), image.height())
    }

    #[test]
    fn scales_down_to_the_longest_edge_and_keeps_the_shape() {
        let wide = DynamicImage::ImageRgb8(RgbImage::from_pixel(3000, 1500, Rgb([9, 9, 9])));
        let out = normalize_image(&encoded(wide, ImageFormat::Png), &ImagePolicy::default());
        let out = out.unwrap();
        assert_eq!((out.width, out.height), (2048, 1024));
        assert_eq!(size(&out.bytes), (2048, 1024));
        assert_eq!(out.media_type, "image/jpeg");
        assert_eq!(size(&out.thumbnail.unwrap()), (512, 256));
    }

    #[test]
    fn keeps_transparency_and_a_small_image_its_size() {
        let clear = DynamicImage::ImageRgba8(RgbaImage::from_pixel(40, 30, Rgba([1, 2, 3, 0])));
        let out = normalize_image(&encoded(clear, ImageFormat::Png), &ImagePolicy::default());
        let out = out.unwrap();
        assert_eq!(
            (out.width, out.height, out.media_type),
            (40, 30, "image/png")
        );
        assert_eq!(out.thumbnail, None);
        let back = image::load_from_memory(&out.bytes).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn turns_a_rotated_photograph_upright_and_drops_its_metadata() {
        // A 2x1 JPEG whose EXIF says it was taken with the camera on its side.
        let flat = DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 1, Rgb([200, 10, 10])));
        let mut jpeg = encoded(flat, ImageFormat::Jpeg);
        let exif: &[u8] = &[
            0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'M', b'M', 0, 42, 0, 0, 0, 8, 0,
            1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0,
        ];
        jpeg.splice(2..2, exif.iter().copied());
        let out = normalize_image(&jpeg, &ImagePolicy::default()).unwrap();
        assert_eq!((out.width, out.height), (1, 2));
        assert!(!out.bytes.windows(4).any(|window| window == b"Exif"));
    }

    #[test]
    fn leaves_alone_what_it_cannot_or_should_not_re_encode() {
        let frame = DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 4, Rgba([1, 2, 3, 255])));
        let policy = ImagePolicy::default();
        assert_eq!(
            normalize_image(&encoded(frame, ImageFormat::Gif), &policy),
            None
        );
        assert_eq!(normalize_image(b"not an image", &policy), None);
    }
}
