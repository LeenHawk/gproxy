//! Explicit image resource operations are separate from ordinary pure conversion.
use super::super::{
    GenerationResources, GenerationStateAccess, image_resources::ImageResourceProgress,
};
use super::*;
use crate::capability::{ResourceAccess, StateStore};
mod gemini;
mod responses;

pub(super) fn wants_uri(input: &g::GenerateContentRequestBody) -> bool {
    input
        .generation_config
        .as_ref()
        .and_then(|config| config.response_format.as_ref())
        .and_then(|format| format.image.as_ref())
        .and_then(|image| image.delivery.as_ref())
        == Some(&g::Delivery::Uri)
}
fn inline_request(mut input: g::GenerateContentRequestBody) -> g::GenerateContentRequestBody {
    if let Some(image) = input
        .generation_config
        .as_mut()
        .and_then(|config| config.response_format.as_mut())
        .and_then(|format| format.image.as_mut())
        && image.delivery == Some(g::Delivery::Uri)
    {
        image.delivery = Some(g::Delivery::Inline);
    }
    input
}
