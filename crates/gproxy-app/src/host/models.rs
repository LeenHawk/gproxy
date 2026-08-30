use gproxy_core::DiscoveredModel;
use gproxy_store::records::{ProviderModelInput, ProviderModelRecord};

use super::AppHost;

/// Fold what an upstream reported into the operator's list.
///
/// Additive by contract: an unseen id is inserted, a field the operator left empty
/// is filled from the wire, and anything they set is left alone. A model that stops
/// being reported is not deleted — an upstream going quiet is not a decision to
/// stop serving it.
pub(super) async fn persist(
    host: &AppHost,
    provider_id: i64,
    models: &[DiscoveredModel],
) -> Result<(), gproxy_store::StoreError> {
    if models.is_empty() {
        return Ok(());
    }
    let snapshot = host.services.control.current();
    let existing = snapshot
        .provider_models
        .iter()
        .filter(|row| row.provider_id == provider_id)
        .map(|row| (row.model_id.as_str(), row))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut changed = false;
    for model in models {
        match existing.get(model.model_id.as_str()) {
            Some(row) => {
                if let Some(input) = filled(row, model) {
                    host.services
                        .store
                        .update_provider_model(row.id, &input)
                        .await?;
                    changed = true;
                }
            }
            None => {
                host.services
                    .store
                    .insert_provider_model(&ProviderModelInput {
                        provider_id,
                        model_id: model.model_id.clone(),
                        display_name: model.display_name.clone(),
                        variants: None,
                        context_window: model.context_window,
                        max_output_tokens: model.max_output_tokens,
                        thinking_supported: None,
                        thinking_adaptive_supported: None,
                        thinking_enabled_supported: None,
                        enabled: true,
                    })
                    .await?;
                changed = true;
            }
        }
    }
    if changed {
        host.services.control.reload().await?;
    }
    Ok(())
}

/// Returns an update only when the wire fills a gap the operator left; a row whose
/// fields are all set produces `None` so an unchanged catalogue costs no writes.
fn filled(row: &ProviderModelRecord, model: &DiscoveredModel) -> Option<ProviderModelInput> {
    let display_name = row
        .display_name
        .clone()
        .or_else(|| model.display_name.clone());
    let context_window = row.context_window.or(model.context_window);
    let max_output_tokens = row.max_output_tokens.or(model.max_output_tokens);
    if display_name == row.display_name
        && context_window == row.context_window
        && max_output_tokens == row.max_output_tokens
    {
        return None;
    }
    Some(ProviderModelInput {
        provider_id: row.provider_id,
        model_id: row.model_id.clone(),
        display_name,
        variants: row.variants.clone(),
        context_window,
        max_output_tokens,
        thinking_supported: row.thinking_supported,
        thinking_adaptive_supported: row.thinking_adaptive_supported,
        thinking_enabled_supported: row.thinking_enabled_supported,
        enabled: row.enabled,
    })
}
