use super::*;
use crate::media_support::{compatibility, warn};
fn tier(value: &ResolutionTier) -> Option<u32> {
    match value {
        ResolutionTier::P512 => Some(512),
        ResolutionTier::K1 => Some(1024),
        ResolutionTier::K2 => Some(2048),
        ResolutionTier::K4 => Some(4096),
        ResolutionTier::Unknown(_) => None,
    }
}
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}
fn ratio(width: u32, height: u32, warnings: &mut Vec<Warning>) -> String {
    let divisor = gcd(width, height);
    let wanted = (width / divisor, height / divisor);
    // upstream_docs/gemini/docs/Generating content.md:4485: supported aspect ratios.
    let supported = [
        (1u32, 1u32),
        (1, 4),
        (4, 1),
        (1, 8),
        (8, 1),
        (2, 3),
        (3, 2),
        (3, 4),
        (4, 3),
        (4, 5),
        (5, 4),
        (9, 16),
        (16, 9),
        (21, 9),
    ];
    let target = f64::from(width) / f64::from(height);
    let selected = supported.into_iter().fold((1u32, 1u32), |best, next| {
        let error = |(a, b): (u32, u32)| (f64::from(a) / f64::from(b) - target).abs();
        if error(next) < error(best) {
            next
        } else {
            best
        }
    });
    if selected != wanted {
        compatibility(
            warnings,
            "aspect_ratio",
            "The requested ratio uses Gemini's nearest documented aspect ratio",
        );
    }
    format!("{}:{}", selected.0, selected.1)
}

pub(super) fn openai(request: &ImageRequest, warnings: &mut Vec<Warning>) -> Option<String> {
    if let Some(size) = request.size {
        return Some(format!("{}x{}", size.width, size.height));
    }
    let edge = request.resolution.as_ref().and_then(tier);
    if request
        .resolution
        .as_ref()
        .is_some_and(|r| matches!(r, ResolutionTier::Unknown(_)))
    {
        warn(warnings, "resolution", "Unknown resolution tier is omitted");
    }
    if edge.is_none() && request.aspect_ratio.is_none() {
        return None;
    }
    let edge = edge.unwrap_or(1024);
    if request.resolution.is_none() {
        compatibility(
            warnings,
            "aspect_ratio",
            "OpenAI needs pixel dimensions; aspect-only requests use a 1K long edge",
        );
    }
    let (a, b) = request
        .aspect_ratio
        .map(|r| (r.width.get(), r.height.get()))
        .unwrap_or((1, 1));
    let (w, h) = if a >= b {
        (
            f64::from(edge),
            f64::from(edge) * f64::from(b) / f64::from(a),
        )
    } else {
        (
            f64::from(edge) * f64::from(a) / f64::from(b),
            f64::from(edge),
        )
    };
    // Create image.md: dimensions must be divisible by 16 for arbitrary sizes.
    let round = |n: f64| ((n / 16.0).round().max(1.0) as u32) * 16;
    let (width, height) = (round(w), round(h));
    if f64::from(width) != w || f64::from(height) != h {
        compatibility(
            warnings,
            "aspect_ratio",
            "Pixel dimensions were rounded to multiples of 16",
        );
    }
    Some(format!("{width}x{height}"))
}
pub(super) fn gemini(
    request: &ImageRequest,
    warnings: &mut Vec<Warning>,
) -> (Option<String>, Option<String>) {
    if let Some(size) = request.size {
        let edge = size.width.get().max(size.height.get());
        let selected = [1024u32, 2048, 4096]
            .into_iter()
            .fold(512u32, |best, value| {
                if value.abs_diff(edge) < best.abs_diff(edge) {
                    value
                } else {
                    best
                }
            });
        compatibility(
            warnings,
            "size",
            "Gemini has ratio/tier controls rather than exact pixel dimensions; exact size is approximated",
        );
        return (
            Some(ratio(size.width.get(), size.height.get(), warnings)),
            Some(
                match selected {
                    512 => "512",
                    1024 => "1K",
                    2048 => "2K",
                    _ => "4K",
                }
                .into(),
            ),
        );
    }
    let resolution = request
        .resolution
        .as_ref()
        .and_then(|r| match r {
            ResolutionTier::P512 => Some("512"),
            ResolutionTier::K1 => Some("1K"),
            ResolutionTier::K2 => Some("2K"),
            ResolutionTier::K4 => Some("4K"),
            ResolutionTier::Unknown(_) => {
                warn(warnings, "resolution", "Unknown resolution tier is omitted");
                None
            }
        })
        .map(str::to_owned);
    (
        request
            .aspect_ratio
            .map(|r| ratio(r.width.get(), r.height.get(), warnings)),
        resolution,
    )
}
