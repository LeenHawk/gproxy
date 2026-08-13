/// Add audio passthrough cells to providers seeded before the audio operations
/// existed. Existing cells remain user-owned and are never overwritten.
pub(super) const SQL: &[&str] = &[
    "INSERT INTO routing_rules \
     (provider_id, operation, kind, implementation, dest_operation, dest_kind, \
      sort_order, enabled, created_at, updated_at) \
     SELECT p.id, 'create_speech', 'open_ai', 'passthrough', NULL, NULL, \
            COALESCE((SELECT MAX(r.sort_order) + 1 FROM routing_rules r \
                      WHERE r.provider_id = p.id), 0), TRUE, 0, 0 \
     FROM providers p \
     WHERE p.channel IN ('openai', 'openrouter', 'custom') \
       AND NOT EXISTS (SELECT 1 FROM routing_rules r WHERE r.provider_id = p.id \
                       AND r.operation = 'create_speech' AND r.kind = 'open_ai')",
    "INSERT INTO routing_rules \
     (provider_id, operation, kind, implementation, dest_operation, dest_kind, \
      sort_order, enabled, created_at, updated_at) \
     SELECT p.id, 'create_transcription', 'open_ai', 'passthrough', NULL, NULL, \
            COALESCE((SELECT MAX(r.sort_order) + 1 FROM routing_rules r \
                      WHERE r.provider_id = p.id), 0), TRUE, 0, 0 \
     FROM providers p \
     WHERE p.channel IN ('openai', 'openrouter', 'custom') \
       AND NOT EXISTS (SELECT 1 FROM routing_rules r WHERE r.provider_id = p.id \
                       AND r.operation = 'create_transcription' AND r.kind = 'open_ai')",
    "INSERT INTO routing_rules \
     (provider_id, operation, kind, implementation, dest_operation, dest_kind, \
      sort_order, enabled, created_at, updated_at) \
     SELECT p.id, 'create_translation', 'open_ai', 'passthrough', NULL, NULL, \
            COALESCE((SELECT MAX(r.sort_order) + 1 FROM routing_rules r \
                      WHERE r.provider_id = p.id), 0), TRUE, 0, 0 \
     FROM providers p \
     WHERE p.channel IN ('openai', 'custom') \
       AND NOT EXISTS (SELECT 1 FROM routing_rules r WHERE r.provider_id = p.id \
                       AND r.operation = 'create_translation' AND r.kind = 'open_ai')",
];
