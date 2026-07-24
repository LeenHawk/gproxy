pub(super) const SQLITE_SQL: &[&str] = &[
    "UPDATE providers SET settings_json = json_remove(\
        json_set(settings_json, '$.claude_fable_fallbacks', \
            json('[\"claude-opus-4-8\"]')), \
        '$.enable_claude_fable_fallback') \
     WHERE json_extract(settings_json, '$.enable_claude_fable_fallback') = 1 \
       AND json_type(settings_json, '$.claude_fable_fallbacks') IS NULL",
    "UPDATE providers SET settings_json = \
        json_remove(settings_json, '$.enable_claude_fable_fallback') \
     WHERE json_type(settings_json, '$.enable_claude_fable_fallback') IS NOT NULL",
];

pub(super) const POSTGRES_SQL: &[&str] = &[
    "UPDATE providers SET settings_json = jsonb_set(\
        settings_json::jsonb - 'enable_claude_fable_fallback', \
        '{claude_fable_fallbacks}', '[\"claude-opus-4-8\"]'::jsonb)::text \
     WHERE settings_json::jsonb -> 'enable_claude_fable_fallback' = 'true'::jsonb \
       AND NOT (settings_json::jsonb ? 'claude_fable_fallbacks')",
    "UPDATE providers SET settings_json = \
        (settings_json::jsonb - 'enable_claude_fable_fallback')::text \
     WHERE settings_json::jsonb ? 'enable_claude_fable_fallback'",
];

pub(super) const MYSQL_SQL: &[&str] = &[
    "UPDATE providers SET settings_json = JSON_REMOVE(\
        JSON_SET(settings_json, '$.claude_fable_fallbacks', \
            JSON_EXTRACT('[\"claude-opus-4-8\"]', '$')), \
        '$.enable_claude_fable_fallback') \
     WHERE JSON_UNQUOTE(JSON_EXTRACT(settings_json, \
        '$.enable_claude_fable_fallback')) = 'true' \
       AND JSON_CONTAINS_PATH(settings_json, 'one', \
        '$.claude_fable_fallbacks') = 0",
    "UPDATE providers SET settings_json = \
        JSON_REMOVE(settings_json, '$.enable_claude_fable_fallback') \
     WHERE JSON_CONTAINS_PATH(settings_json, 'one', \
        '$.enable_claude_fable_fallback')",
];
