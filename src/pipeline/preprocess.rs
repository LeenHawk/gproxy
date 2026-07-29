//! Aggregated-mode preprocessing: rewrite the requested public model through
//! global aliases only. Route lookup and `provider/model` parsing happen after
//! this step; provider-scoped aliases are applied after the provider is known.

use crate::app::snapshot::ControlPlaneSnapshot;
use crate::pipeline::classify::path_model_id;
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;

/// Return the inbound body/path model after global aliases. This does not
/// resolve routes and does not infer a provider for bare model names.
pub fn preprocess(cp: &ControlPlaneSnapshot, ctx: &RequestCtx) -> Result<String, PipelineError> {
    let model =
        requested_model(ctx).ok_or_else(|| PipelineError::UnknownRoute("<no model>".into()))?;
    Ok(apply_global_alias(cp, &model))
}

/// Apply global aliases only. If nothing matches, the original model remains
/// unchanged; bare names are still bare and will not resolve to a provider
/// unless another config path (for example a route) handles them.
pub fn apply_global_alias(cp: &ControlPlaneSnapshot, model: &str) -> String {
    apply_aliases(cp, "*", model).unwrap_or_else(|| model.to_owned())
}

/// Body model (classify's single peek) falling back to the path-embedded
/// model. Runs post-classify, so no body re-parse happens here.
pub fn requested_model(ctx: &RequestCtx) -> Option<String> {
    ctx.body_model.clone().or_else(|| path_model_id(&ctx.path))
}

fn apply_aliases(cp: &ControlPlaneSnapshot, provider: &str, model: &str) -> Option<String> {
    cp.aliases_by_provider
        .get(provider)?
        .iter()
        .find_map(|alias| alias.apply(model))
}

pub fn apply_provider_alias(cp: &ControlPlaneSnapshot, provider_name: &str, model: &str) -> String {
    apply_aliases(cp, provider_name, model).unwrap_or_else(|| model.to_owned())
}

pub fn split_provider_model(model: &str) -> Option<(&str, &str)> {
    let (provider, rest) = model.split_once('/')?;
    (!provider.is_empty() && !rest.is_empty()).then_some((provider, rest))
}
