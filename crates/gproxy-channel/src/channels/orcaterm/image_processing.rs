//! Match the desktop's attachment preparation (features.js:32194): preserve
//! images already within bounds, resize static images to a 4096-pixel long
//! edge, retain PNG transparency, and try JPEG quality 90 then 70. GIFs remain
//! animated and are not recompressed.
use super::request::bad;
use crate::channel::ChannelError;
use image::{
    ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    codecs::{
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType, PngEncoder},
    },
};
use std::io::Cursor;

pub(super) const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

pub(super) fn prepare(
    bytes: Vec<u8>,
    mime: &'static str,
) -> Result<(Vec<u8>, &'static str, &'static str), ChannelError> {
    let (format, extension) = match mime {
        "image/png" => (ImageFormat::Png, "png"),
        "image/jpeg" => (ImageFormat::Jpeg, "jpg"),
        "image/webp" => (ImageFormat::WebP, "webp"),
        "image/gif" => (ImageFormat::Gif, "gif"),
        _ => return Err(bad("unsupported image type")),
    };
    let (width, height) = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| bad("invalid image"))?;
    if format == ImageFormat::Gif {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(bad("GIF exceeds 10 MiB and cannot be compressed"));
        }
        return Ok((bytes, mime, extension));
    }
    if width <= 4096 && height <= 4096 && bytes.len() <= MAX_FILE_BYTES {
        return Ok((bytes, mime, extension));
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| bad("image cannot be decoded within limits"))?;
    let orientation = decoder
        .orientation()
        .map_err(|_| bad("invalid image orientation"))?;
    let mut image = image::DynamicImage::from_decoder(decoder)
        .map_err(|_| bad("image cannot be decoded within limits"))?;
    image.apply_orientation(orientation);
    if image.width() > 4096 || image.height() > 4096 {
        image = image.resize(4096, 4096, image::imageops::FilterType::Triangle);
    }
    let mut output = Vec::new();
    let (mime, extension) = if format == ImageFormat::Png {
        let pixels = image.into_rgba8();
        PngEncoder::new_with_quality(&mut output, CompressionType::Best, FilterType::Adaptive)
            .write_image(
                pixels.as_raw(),
                pixels.width(),
                pixels.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|_| bad("image compression failed"))?;
        ("image/png", "png")
    } else {
        let pixels = if image.color().has_alpha() {
            let rgba = image.into_rgba8();
            image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
                let p = rgba.get_pixel(x, y);
                let a = u16::from(p[3]);
                image::Rgb(
                    [0, 1, 2].map(|i| ((u16::from(p[i]) * a + 255 * (255 - a) + 127) / 255) as u8),
                )
            })
        } else {
            image.into_rgb8()
        };
        for quality in [90, 70] {
            output.clear();
            JpegEncoder::new_with_quality(&mut output, quality)
                .encode_image(&pixels)
                .map_err(|_| bad("image compression failed"))?;
            if output.len() <= MAX_FILE_BYTES {
                break;
            }
        }
        ("image/jpeg", "jpg")
    };
    if output.len() > MAX_FILE_BYTES {
        return Err(bad("image still exceeds 10 MiB after compression"));
    }
    Ok((output, mime, extension))
}
