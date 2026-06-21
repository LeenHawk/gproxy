export interface SanitizeTemplate {
  id: string;
  pattern: string;
  replacement: string;
}

// Word-boundary client-identity scrubs. Each is a single sanitize rule the user
// fills into the sanitize editor (then can tweak before saving).
export const SANITIZE_TEMPLATES: SanitizeTemplate[] = [
  { id: "aider", pattern: "\\bAider\\b", replacement: "The assistant" },
  { id: "cline", pattern: "\\bCline\\b", replacement: "Assistant" },
  { id: "continue", pattern: "\\bContinue\\b", replacement: "Assistant" },
  { id: "cursor", pattern: "\\bCursor\\b", replacement: "Assistant" },
  // OpenCode: its <env> block writes "git repo" (abbreviated) where the official
  // Claude Code client writes "git repository" — the upstream's client-identity
  // check flags the abbreviation as a third-party app. Confirmed against the live
  // upstream 2026-06-20 (raw "git repo" → 400; "git repository" → 200).
  { id: "opencode", pattern: "\\bgit repo\\b", replacement: "git repository" },
];
