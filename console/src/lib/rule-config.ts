type RuleConfigObject = Record<string, unknown>;

function objectConfig(value: unknown): RuleConfigObject {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as RuleConfigObject)
    : {};
}

function isObjectValue(value: unknown): value is RuleConfigObject {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

/**
 * Materialized defaults for every structured rule editor. These values are
 * both displayed and submitted; controls must not invent a visual-only
 * fallback that is absent from config_json.
 */
export function defaultRuleConfig(kind: string): RuleConfigObject {
  switch (kind) {
    case "system_text":
      return { text: "", position: "prepend" };
    case "cache_breakpoint":
      return { target: "system" };
    case "rewrite":
      return { path: "", action: "set", value_json: null };
    case "header":
      return { name: "", value: "", mode: "override" };
    default:
      return {};
  }
}

/** Normalize new and legacy configs before editing or saving. */
export function normalizeRuleConfig(kind: string, value: unknown): RuleConfigObject {
  const normalized = { ...defaultRuleConfig(kind), ...objectConfig(value) };

  if (kind === "rewrite") {
    if (normalized.action === "delete") {
      delete normalized.value_json;
    } else if (normalized.action === "merge" && !isObjectValue(normalized.value_json)) {
      normalized.value_json = {};
    } else if (!("value_json" in normalized)) {
      normalized.value_json = null;
    }
  }

  return normalized;
}
