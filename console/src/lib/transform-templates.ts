export interface TransformTemplate {
  id: string;
  config: {
    phase: "request";
    locate: { match: string } | { paths: string[] };
    actions: { op: "replace_text"; from?: string; with: string }[];
  };
}

function replaceText(id: string, pattern: string, replacement: string): TransformTemplate {
  return {
    id,
    config: {
      phase: "request",
      locate: { match: pattern },
      actions: [{ op: "replace_text", with: replacement }],
    },
  };
}

function replacePathTexts(
  id: string,
  paths: readonly string[],
  replacements: readonly (readonly [string, string])[],
): TransformTemplate {
  return {
    id,
    config: {
      phase: "request",
      locate: { paths: [...paths] },
      actions: replacements.map(([from, to]) => ({ op: "replace_text", from, with: to })),
    },
  };
}

export const OPENCODE_REQUEST_TOOL_PATHS = [
  "tools.*.name",
  "tool_choice.name",
  "messages.*.content.*.name",
  "messages.*.content.*.tool_name",
  "messages.*.content.*.content.*.tool_name",
] as const;

export const OPENCODE_TOOL_RENAMES = [
  ["bash", "Bash"],
  ["read", "Read"],
  ["write", "Write"],
  ["edit", "Edit"],
  ["glob", "Glob"],
  ["grep", "Grep"],
  ["task", "Task"],
  ["webfetch", "WebFetch"],
  ["todowrite", "TodoWrite"],
  ["question", "Question"],
  ["skill", "Skill"],
  ["ls", "LS"],
  ["todoread", "TodoRead"],
  ["notebookedit", "NotebookEdit"],
] as const;

// Client-identity downgrades. These are UI presets only; the backend still runs
// the explicit transform JSON saved by the user.
export const TRANSFORM_TEMPLATES: TransformTemplate[] = [
  replaceText("aider", "\\bAider\\b", "The assistant"),
  replaceText("cline", "\\bCline\\b", "Assistant"),
  replaceText("continue", "\\bContinue\\b", "Assistant"),
  replaceText("cursor", "\\bCursor\\b", "Assistant"),
  // OpenCode's environment block uses "git repo"; Claude Code says
  // "git repository". The shorter phrase is a known OAuth third-party trigger.
  replaceText("opencode-env", "\\bgit repo\\b", "git repository"),
  // OpenCode emits lowercase tool names. Claude Code OAuth traffic uses these
  // TitleCase names; keeping tools but renaming them avoids third-party
  // tool-fingerprint triggers.
  replacePathTexts(
    "opencode-tools",
    OPENCODE_REQUEST_TOOL_PATHS,
    OPENCODE_TOOL_RENAMES,
  ),
];
