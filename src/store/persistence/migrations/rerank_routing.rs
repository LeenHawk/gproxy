/// Materialize the rerank cell added to the custom and OpenRouter channel
/// defaults for providers whose routing tables were seeded by an older build.
/// The anti-join is deliberate: an existing cell is user-owned, even when it
/// is disabled or uses a non-default implementation.
pub(super) const SQL: &[&str] = &["INSERT INTO routing_rules \
    (provider_id, operation, kind, implementation, dest_operation, dest_kind, \
     sort_order, enabled, created_at, updated_at) \
    SELECT p.id, 'rerank', 'open_ai', 'passthrough', NULL, NULL, \
           COALESCE((SELECT MAX(existing.sort_order) + 1 \
                     FROM routing_rules existing \
                     WHERE existing.provider_id = p.id), 0), \
           TRUE, 0, 0 \
    FROM providers p \
    WHERE p.channel IN ('custom', 'openrouter') \
      AND NOT EXISTS (SELECT 1 FROM routing_rules existing \
                      WHERE existing.provider_id = p.id \
                        AND existing.operation = 'rerank' \
                        AND existing.kind = 'open_ai')"];
