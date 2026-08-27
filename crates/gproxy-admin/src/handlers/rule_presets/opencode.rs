use crate::dto::*;

use super::catalog::transform;

const REQUEST_TOOL_PATHS: &[&str] = &[
    "tools.*.name",
    "tool_choice.name",
    "messages.*.content.*.name",
    "messages.*.content.*.tool_name",
    "messages.*.content.*.content.*.tool_name",
];
const RESPONSE_TOOL_PATHS: &[&str] = &[
    "content.*.name",
    "content.*.tool_name",
    "message.content.*.name",
    "message.content.*.tool_name",
    "content_block.name",
    "content_block.tool_name",
];
const TOOL_RENAMES: &[(&str, &str)] = &[
    ("bash", "Bash"),
    ("read", "Read"),
    ("write", "Write"),
    ("edit", "Edit"),
    ("glob", "Glob"),
    ("grep", "Grep"),
    ("task", "Task"),
    ("webfetch", "WebFetch"),
    ("todowrite", "TodoWrite"),
    ("question", "Question"),
    ("skill", "Skill"),
    ("ls", "LS"),
    ("todoread", "TodoRead"),
    ("notebookedit", "NotebookEdit"),
];
const CLIENT: &str = "^user-agent: opencode/";

pub(super) fn preset() -> RulePresetDto {
    RulePresetDto {
        id: "opencode".into(),
        name: "OpenCode".into(),
        description: "gproxy:preset:opencode:v1".into(),
        category: RulePresetCategoryDto::Application,
        rules: vec![
            transform(
                0,
                TransformPhaseDto::Request,
                &["system", "system.*.text"],
                vec![
                    regex(
                        "(?s)Here is some useful information about the environment you are running in:\\s*<env>.*?</env>\\n?",
                        "",
                    ),
                    regex(
                        "(?i)https://github\\.com/anomalyco/opencode(?:/[^\\s)]*)?",
                        "the project issue tracker",
                    ),
                    regex(
                        "(?i)https://opencode\\.ai/docs(?:/[^\\s)]*)?",
                        "the documentation",
                    ),
                    regex(
                        "(?i)(?:~/)?\\.config/opencode/|\\.opencode/",
                        "the assistant config directory/",
                    ),
                    regex("(?i)/tmp/opencode\\b", "/tmp/coding-agent"),
                    regex("(?i)\\bopencode\\b", "the coding assistant"),
                    regex("\\bgit repo\\b", "git repository"),
                ],
                Some(CLIENT),
            ),
            transform(
                1,
                TransformPhaseDto::Request,
                REQUEST_TOOL_PATHS,
                renames(false),
                Some(CLIENT),
            ),
            transform(
                2,
                TransformPhaseDto::Request,
                REQUEST_TOOL_PATHS,
                vec![regex("^mcp_([^_].*)$", "mcp__$1")],
                Some(CLIENT),
            ),
            transform(
                3,
                TransformPhaseDto::Response,
                RESPONSE_TOOL_PATHS,
                renames(true),
                Some(CLIENT),
            ),
            transform(
                4,
                TransformPhaseDto::Response,
                RESPONSE_TOOL_PATHS,
                vec![regex("^mcp__([^_].*)$", "mcp_$1")],
                Some(CLIENT),
            ),
        ],
    }
}

fn regex(pattern: &str, with: &str) -> TransformActionDto {
    TransformActionDto::ReplaceRegex {
        pattern: pattern.into(),
        with: with.into(),
    }
}

fn renames(reverse: bool) -> Vec<TransformActionDto> {
    TOOL_RENAMES
        .iter()
        .map(|(original, renamed)| {
            let (from, with) = if reverse {
                (*renamed, *original)
            } else {
                (*original, *renamed)
            };
            TransformActionDto::ReplaceText {
                from: Some(from.into()),
                with: with.into(),
            }
        })
        .collect()
}
