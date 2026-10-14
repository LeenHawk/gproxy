import type { RewriteRuleWrite } from "@/generated/sdk"

export type ConditionDraft = { key: string; op: "eq" | "ne" | "exists" | "not_exists"; value: string }
type Condition = Exclude<NonNullable<RewriteRuleWrite["filterBody"]>, string> | NonNullable<RewriteRuleWrite["filterHeader"]>
export function conditionDraft(condition: Condition | null | undefined): ConditionDraft {
  return { key: condition ? "path" in condition ? condition.path : condition.name : "", op: condition?.op ?? "eq", value: condition && "value" in condition ? "path" in condition ? JSON.stringify(condition.value) : condition.value ?? "" : "" }
}
export type BodyConditionDraft = { mode: "fields" | "jmespath"; expression: string; fields: ConditionDraft }
export function bodyConditionDraft(condition: RewriteRuleWrite["filterBody"] | undefined): BodyConditionDraft {
  return typeof condition === "string"
    ? { mode: "jmespath", expression: condition, fields: conditionDraft(null) }
    : { mode: condition ? "fields" : "jmespath", expression: "", fields: conditionDraft(condition) }
}
export function bodyCondition(draft: BodyConditionDraft): RewriteRuleWrite["filterBody"] {
  if (draft.mode === "jmespath") return draft.expression.trim() || null
  const { key, op, value } = draft.fields
  if (!key.trim()) return null
  return { path: key.trim(), op, ...(op === "eq" || op === "ne" ? { value: JSON.parse(value) as unknown } : {}) }
}
export function headerCondition(draft: ConditionDraft): RewriteRuleWrite["filterHeader"] {
  if (!draft.key.trim()) return null
  return { name: draft.key.trim(), op: draft.op, ...(draft.op === "eq" || draft.op === "ne" ? { value: draft.value } : {}) }
}
