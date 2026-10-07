import type { RewriteRuleWrite } from "@/generated/sdk"

export type RuleJson = Omit<RewriteRuleWrite, "id" | "ruleSetId" | "sortOrder">

const defaults: RuleJson = {
  action: "replace", phase: "request", target: "body", targetName: null,
  paths: null, pattern: "", replacement: "", filterOperationKeys: null,
  filterModelPattern: null, filterHeaderPattern: null, filterEventPattern: null,
  filterBody: null, filterHeader: null,
  enabled: true,
}

export function ruleJson(write: RewriteRuleWrite): RuleJson {
  return Object.fromEntries(Object.keys(defaults).map(key => [key, write[key as keyof RuleJson]])) as RuleJson
}

/** Check the editable JSON shape here; the server compiles and validates the rule. */
export function parseRuleJson(text: string, invalid: (field: string) => string): RuleJson {
  const value: unknown = JSON.parse(text)
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(invalid("JSON"))
  const fields = value as Record<string, unknown>
  for (const key of Object.keys(fields)) {
    if (!Object.hasOwn(defaults, key)) throw new Error(invalid(key))
  }
  const rule = {
    ...defaults, ...fields,
    action: fields.action ?? defaults.action, phase: fields.phase ?? defaults.phase,
    target: fields.target ?? defaults.target, enabled: fields.enabled ?? defaults.enabled,
  } as RuleJson
  for (const key of ["action", "phase", "target", "targetName", "filterModelPattern", "filterHeaderPattern", "filterEventPattern"] as const) {
    if (rule[key] !== null && typeof rule[key] !== "string") throw new Error(invalid(key))
  }
  for (const key of ["pattern", "replacement"] as const) {
    if (typeof rule[key] !== "string") throw new Error(invalid(key))
  }
  const choices = {
    action: ["replace", "set", "delete", "merge", "header_set", "header_merge", "system_text", "cache_breakpoint"],
    phase: ["request", "response", "both"], target: ["body", "header", "query"],
  }
  for (const key of ["action", "phase", "target"] as const) {
    if (rule[key] !== null && !choices[key].includes(rule[key])) throw new Error(invalid(key))
  }
  if (rule.enabled !== null && typeof rule.enabled !== "boolean") throw new Error(invalid("enabled"))
  if (rule.paths !== null && (!Array.isArray(rule.paths) || rule.paths.some(path => typeof path !== "string"))) throw new Error(invalid("paths"))
  if (rule.filterOperationKeys !== null && (!Array.isArray(rule.filterOperationKeys) || rule.filterOperationKeys.some(key => !key || typeof key.operation !== "string" || typeof key.dialect !== "string"))) throw new Error(invalid("filterOperationKeys"))
  for (const key of ["filterBody", "filterHeader"] as const) {
    const condition = rule[key]
    if (condition === null) continue
    if (key === "filterBody" && typeof condition === "string") {
      if (!condition.trim()) throw new Error(invalid(key))
      continue
    }
    const field = key === "filterBody" ? "path" : "name"
    if (!condition || typeof condition !== "object" || Array.isArray(condition)) throw new Error(invalid(key))
    const object = condition as unknown as Record<string, unknown>
    if (Object.keys(object).some(k => ![field, "op", "value"].includes(k)) || typeof object[field] !== "string" || !object[field].trim() || !["eq", "ne", "exists", "not_exists"].includes(String(object.op))) throw new Error(invalid(key))
    const compares = object.op === "eq" || object.op === "ne"
    if (compares !== Object.hasOwn(object, "value") || (compares && key === "filterHeader" && typeof object.value !== "string")) throw new Error(invalid(key))
  }
  return rule as RuleJson
}
