/// Constants and types shared across provider settings editors.

// ---------------------------------------------------------------------------
// Cache breakpoint types
// ---------------------------------------------------------------------------

export type CacheBreakpointTarget = "top_level" | "tools" | "system" | "messages";
export type CacheBreakpointPosition = "nth" | "last_nth";
export type CacheBreakpointTtl = "auto" | "5m" | "1h";

export type CacheBreakpointRule = {
  target: CacheBreakpointTarget;
  position: CacheBreakpointPosition;
  index: number;
  ttl: CacheBreakpointTtl;
};

export function parseCacheBreakpoints(value: unknown): CacheBreakpointRule[] {
  let source: unknown = value;
  if (typeof value === "string") {
    try {
      source = JSON.parse(value);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(source)) {
    return [];
  }
  return source
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => ({
      target: normalizeTarget(item.target),
      position: normalizePosition(item.position),
      index: normalizeIndex(item.index),
      ttl: normalizeTtl(item.ttl),
    }))
    .slice(0, 4);
}

function normalizeTarget(value: unknown): CacheBreakpointTarget {
  if (typeof value === "string") {
    const v = value.trim().toLowerCase();
    if (v === "top_level" || v === "global") return "top_level";
    if (v === "tools") return "tools";
    if (v === "system") return "system";
  }
  return "messages";
}

function normalizePosition(value: unknown): CacheBreakpointPosition {
  if (typeof value === "string" && value.trim().toLowerCase() === "last_nth") return "last_nth";
  return "nth";
}

function normalizeTtl(value: unknown): CacheBreakpointTtl {
  if (typeof value === "string") {
    const v = value.trim().toLowerCase();
    if (v === "5m") return "5m";
    if (v === "1h") return "1h";
  }
  return "auto";
}

function normalizeIndex(value: unknown): number {
  if (typeof value === "number" && Number.isFinite(value)) return Math.max(1, Math.trunc(value));
  if (typeof value === "string") {
    const n = Number(value.trim());
    if (Number.isFinite(n)) return Math.max(1, Math.trunc(n));
  }
  return 1;
}

export const RECOMMENDED_CACHE_TEMPLATE: CacheBreakpointRule[] = [
  { target: "system", position: "last_nth", index: 1, ttl: "auto" },
  { target: "messages", position: "last_nth", index: 11, ttl: "auto" },
  { target: "messages", position: "last_nth", index: 2, ttl: "auto" },
  { target: "messages", position: "last_nth", index: 1, ttl: "auto" },
];

// ---------------------------------------------------------------------------
// Beta headers
// ---------------------------------------------------------------------------

export const ANTHROPIC_REFERENCE_BETA_HEADERS = [
  "message-batches-2024-09-24",
  "prompt-caching-2024-07-31",
  "computer-use-2024-10-22",
  "computer-use-2025-01-24",
  "pdfs-2024-09-25",
  "token-counting-2024-11-01",
  "token-efficient-tools-2025-02-19",
  "output-128k-2025-02-19",
  "files-api-2025-04-14",
  "mcp-client-2025-04-04",
  "mcp-client-2025-11-20",
  "dev-full-thinking-2025-05-14",
  "interleaved-thinking-2025-05-14",
  "code-execution-2025-05-22",
  "extended-cache-ttl-2025-04-11",
  "context-1m-2025-08-07",
  "context-management-2025-06-27",
  "model-context-window-exceeded-2025-08-26",
  "skills-2025-10-02",
  "fast-mode-2026-02-01",
  "compact-2026-01-12",
  "claude-code-20250219",
  "adaptive-thinking-2026-01-28",
  "prompt-caching-scope-2026-01-05",
  "advanced-tool-use-2025-11-20",
  "effort-2025-11-24",
] as const;

export const CLAUDECODE_OAUTH_BETA = "oauth-2025-04-20";

export function parseBetaHeaders(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === "string" && item.trim() !== "");
  }
  if (typeof value === "string") {
    try {
      const parsed: unknown = JSON.parse(value);
      if (!Array.isArray(parsed)) {
        return [];
      }
      return parsed.filter((item): item is string => typeof item === "string" && item.trim() !== "");
    } catch {
      return [];
    }
  }
  return [];
}

// ---------------------------------------------------------------------------
// Sanitize rules
// ---------------------------------------------------------------------------

export type SanitizeRule = {
  pattern: string;
  replacement: string;
};

export function parseSanitizeRules(value: unknown): SanitizeRule[] {
  let source: unknown = value;
  if (typeof value === "string") {
    try {
      source = JSON.parse(value);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(source)) {
    return [];
  }
  return source
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => ({
      pattern: typeof item.pattern === "string" ? item.pattern : "",
      replacement: typeof item.replacement === "string" ? item.replacement : "",
    }));
}

// ---------------------------------------------------------------------------
// Prelude templates
// ---------------------------------------------------------------------------

export const CLAUDE_CODE_PRELUDE =
  "You are Claude Code, Anthropic's official CLI for Claude.";

export const CLAUDE_AGENT_SDK_PRELUDE =
  "You are a Claude agent, built on Anthropic's Claude Agent SDK.";

// ---------------------------------------------------------------------------
// Sanitize rule templates
// ---------------------------------------------------------------------------

/// Pre-built sanitize rule sets for common third-party CLI clients that
/// trigger upstream subscription-pool client-identification errors.
/// Operators can pick a template and optionally customize rules after.
export const SANITIZE_TEMPLATES: Array<{
  key: string;
  label: string;
  rules: SanitizeRule[];
}> = [
  {
    key: "pi",
    label: "pi-mono",
    rules: [
      { pattern: "\\bPi documentation\\b", replacement: "Harness documentation" },
      { pattern: "\\binside pi, a coding\\b", replacement: "inside the coding" },
      { pattern: "\\bpi packages\\b", replacement: "harness packages" },
      { pattern: "\\bpi topics\\b", replacement: "harness topics" },
      { pattern: "\\bpi \\.md files\\b", replacement: "the harness .md files" },
      { pattern: "\\bpi itself\\b", replacement: "the harness itself" },
      { pattern: "\\bpi\\b", replacement: "the agent" },
      { pattern: "\\bPi\\b", replacement: "The agent" },
      { pattern: "\\bPI\\b", replacement: "AGENT" },
    ],
  },
  {
    key: "aider",
    label: "Aider",
    rules: [
      { pattern: "\\bAider\\b", replacement: "The assistant" },
      { pattern: "\\baider\\b", replacement: "the assistant" },
    ],
  },
  {
    key: "cline",
    label: "Cline",
    rules: [{ pattern: "\\bCline\\b", replacement: "Assistant" }],
  },
  {
    key: "continue",
    label: "Continue",
    rules: [{ pattern: "\\bContinue\\b", replacement: "Assistant" }],
  },
  {
    key: "cursor",
    label: "Cursor",
    rules: [{ pattern: "\\bCursor\\b", replacement: "Assistant" }],
  },
];

// ---------------------------------------------------------------------------
// Rewrite rules
// ---------------------------------------------------------------------------

export type RewriteAction =
  | { type: "Set"; value: unknown }
  | { type: "Remove" };

export type RewriteFilter = {
  model_pattern?: string;
  operations?: string[];
  protocols?: string[];
};

export type RewriteRule = {
  path: string;
  action: RewriteAction;
  filter?: RewriteFilter;
};

export function parseRewriteRules(value: unknown): RewriteRule[] {
  let source: unknown = value;
  if (typeof value === "string") {
    try {
      source = JSON.parse(value);
    } catch {
      return [];
    }
  }
  if (!Array.isArray(source)) {
    return [];
  }
  return source
    .filter((item): item is Record<string, unknown> => !!item && typeof item === "object")
    .map((item) => {
      const action = normalizeRewriteAction(item.action);
      const filter = normalizeRewriteFilter(item.filter);
      return {
        path: typeof item.path === "string" ? item.path : "",
        action,
        ...(filter ? { filter } : {}),
      };
    });
}

function normalizeRewriteAction(value: unknown): RewriteAction {
  if (value && typeof value === "object" && "type" in value) {
    const obj = value as Record<string, unknown>;
    if (obj.type === "Remove") return { type: "Remove" };
    if (obj.type === "Set") return { type: "Set", value: obj.value ?? null };
  }
  return { type: "Set", value: null };
}

function normalizeRewriteFilter(value: unknown): RewriteFilter | undefined {
  if (!value || typeof value !== "object") return undefined;
  const obj = value as Record<string, unknown>;
  const filter: RewriteFilter = {};
  if (typeof obj.model_pattern === "string" && obj.model_pattern) {
    filter.model_pattern = obj.model_pattern;
  }
  if (Array.isArray(obj.operations) && obj.operations.length > 0) {
    filter.operations = obj.operations.filter((v): v is string => typeof v === "string");
  }
  if (Array.isArray(obj.protocols) && obj.protocols.length > 0) {
    filter.protocols = obj.protocols.filter((v): v is string => typeof v === "string");
  }
  if (!filter.model_pattern && !filter.operations && !filter.protocols) return undefined;
  return filter;
}
