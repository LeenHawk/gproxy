use crate::dto::*;

const REQUEST_TEXT_PATHS: &[&str] = &[
    "system",
    "system.*.text",
    "instructions",
    "messages.*.content",
    "messages.*.content.*.text",
    "messages.*.content.*.content.*.text",
    "input",
    "input.*.content",
    "input.*.content.*.text",
    "contents.*.parts.*.text",
    "systemInstruction.parts.*.text",
];

pub(super) fn all() -> Vec<RulePresetDto> {
    let mut presets = vec![opencode::preset()];
    presets.extend([
        sanitize(
            "pi",
            "pi-mono",
            None,
            &[
                ("\\bPi documentation\\b", "Harness documentation"),
                ("\\binside pi, a coding\\b", "inside the coding"),
                ("\\bpi packages\\b", "harness packages"),
                ("\\bpi topics\\b", "harness topics"),
                ("\\bpi \\.md files\\b", "the harness .md files"),
                ("\\bpi itself\\b", "the harness itself"),
                ("\\bpi\\b", "the agent"),
                ("\\bPi\\b", "The agent"),
                ("\\bPI\\b", "AGENT"),
            ],
        ),
        sanitize(
            "aider",
            "Aider",
            Some("^user-agent: litellm/"),
            &[
                ("\\bAider\\b", "The assistant"),
                ("\\baider\\b", "the assistant"),
            ],
        ),
        sanitize(
            "cline",
            "Cline",
            Some("^user-agent: cline/|^x-title: cline$|^http-referer: https://cline\\.bot"),
            &[("\\bCline\\b", "Assistant")],
        ),
        sanitize(
            "continue",
            "Continue",
            Some("^user-agent: continue/"),
            &[("\\bContinue\\b", "Assistant")],
        ),
        sanitize(
            "cursor",
            "Cursor",
            Some("^user-agent: cursor/"),
            &[("\\bCursor\\b", "Assistant")],
        ),
        cache("claude-cache-system", "Claude system cache", "system"),
        cache("claude-cache-message", "Claude message cache", "message"),
    ]);
    presets
}

fn sanitize(
    id: &str,
    name: &str,
    header: Option<&str>,
    replacements: &[(&str, &str)],
) -> RulePresetDto {
    RulePresetDto {
        id: id.into(),
        name: name.into(),
        description: format!("gproxy:preset:{id}:v1"),
        category: RulePresetCategoryDto::Application,
        rules: replacements
            .iter()
            .enumerate()
            .map(|(index, (pattern, replacement))| {
                transform(
                    index as i64,
                    TransformPhaseDto::Request,
                    REQUEST_TEXT_PATHS,
                    vec![TransformActionDto::ReplaceRegex {
                        pattern: (*pattern).into(),
                        with: (*replacement).into(),
                    }],
                    header,
                )
            })
            .collect(),
    }
}

pub(super) fn transform(
    sort_order: i64,
    phase: TransformPhaseDto,
    paths: &[&str],
    actions: Vec<TransformActionDto>,
    header: Option<&str>,
) -> RulePresetRuleDto {
    RulePresetRuleDto {
        config: RuleConfigDto::Transform {
            phase,
            locate: TransformLocateDto::Paths(paths.iter().map(|value| (*value).into()).collect()),
            actions,
            limit: None,
        },
        filter_model_pattern: None,
        filter_operations: Some(vec![
            "generate_content".into(),
            "stream_generate_content".into(),
        ]),
        filter_header_pattern: header.map(Into::into),
        sort_order,
        enabled: true,
    }
}

fn cache(id: &str, name: &str, target: &str) -> RulePresetDto {
    RulePresetDto {
        id: id.into(),
        name: name.into(),
        description: format!("gproxy:preset:{id}:v1"),
        category: RulePresetCategoryDto::Cache,
        rules: vec![RulePresetRuleDto {
            config: RuleConfigDto::CacheBreakpoint {
                target: target.into(),
                index: None,
                ttl: Some("1h".into()),
            },
            filter_model_pattern: None,
            filter_operations: Some(vec![
                "generate_content".into(),
                "stream_generate_content".into(),
            ]),
            filter_header_pattern: None,
            sort_order: 0,
            enabled: true,
        }],
    }
}

use super::opencode;
