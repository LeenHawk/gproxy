import type { RewriteRuleDto } from "@/generated/sdk"
export const kinds = ["system_text", "cache_breakpoint", "rewrite", "transform", "header"] as const
export const rewriteActions = ["set", "delete", "merge", "json_patch"]
export type RuleKind = typeof kinds[number]
export function ruleKind(rule: Pick<RewriteRuleDto, "action" | "target">): RuleKind {
  if (rule.action === "system_text" || rule.action === "cache_breakpoint") return rule.action
  if (rewriteActions.includes(rule.action)) return "rewrite"
  if (rule.target === "header" && rule.action !== "replace") return "header"
  return "transform"
}
