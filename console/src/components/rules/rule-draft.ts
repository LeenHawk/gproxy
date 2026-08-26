import type { RuleConfigDto } from "@/generated/RuleConfigDto"
import type { RuleDto } from "@/generated/RuleDto"

export type ActionDraft = { op: "replace_text" | "replace_regex"; from: string; pattern: string; with: string }
export type RuleDraft = {
  kind: RuleConfigDto["kind"]
  text: string
  position: "prepend" | "append"
  cacheTarget: string
  cacheIndex: string
  ttl: string
  path: string
  rewriteAction: "set" | "delete" | "merge"
  rewriteValue: string
  phase: "request" | "response" | "both"
  locateType: "path" | "paths" | "match"
  locateValue: string
  actions: Array<ActionDraft>
  limit: string
  headerName: string
  headerValue: string
  headerMode: "override" | "merge"
}

export function ruleDraft(rule?: RuleDto): RuleDraft {
  const base: RuleDraft = {
    kind: "system_text", text: "", position: "prepend", cacheTarget: "system", cacheIndex: "", ttl: "", path: "", rewriteAction: "set", rewriteValue: "null", phase: "request", locateType: "path", locateValue: "", actions: [{ op: "replace_text", from: "", pattern: "", with: "" }], limit: "", headerName: "", headerValue: "", headerMode: "override",
  }
  if (!rule) return base
  const config = rule.config
  base.kind = config.kind
  if (config.kind === "system_text") { base.text = config.text; base.position = config.position }
  if (config.kind === "cache_breakpoint") { base.cacheTarget = config.target; base.cacheIndex = config.index == null ? "" : String(config.index); base.ttl = config.ttl ?? "" }
  if (config.kind === "rewrite") { base.path = config.path; base.rewriteAction = config.action; base.rewriteValue = config.value == null ? "null" : JSON.stringify(config.value, null, 2) }
  if (config.kind === "transform") {
    base.phase = config.phase
    base.locateType = config.locate.type
    base.locateValue = config.locate.type === "paths" ? config.locate.value.join("\n") : config.locate.value
    base.actions = config.actions.map((action) => action.op === "replace_text" ? { op: action.op, from: action.from ?? "", pattern: "", with: action.with } : { op: action.op, from: "", pattern: action.pattern, with: action.with })
    base.limit = config.limit == null ? "" : String(config.limit)
  }
  if (config.kind === "header") { base.headerName = config.name; base.headerValue = config.value; base.headerMode = config.mode }
  return base
}

export function configFromDraft(draft: RuleDraft): RuleConfigDto {
  if (draft.kind === "system_text") return { kind: draft.kind, text: draft.text, position: draft.position }
  if (draft.kind === "cache_breakpoint") return { kind: draft.kind, target: draft.cacheTarget, index: draft.cacheIndex === "" ? null : Number(draft.cacheIndex), ttl: draft.ttl || null }
  if (draft.kind === "rewrite") return { kind: draft.kind, path: draft.path, action: draft.rewriteAction, value: draft.rewriteAction === "delete" ? null : JSON.parse(draft.rewriteValue) as unknown }
  if (draft.kind === "transform") {
    const locateType = draft.locateType || "path"
    const locate = locateType === "paths" ? { type: "paths" as const, value: draft.locateValue.split("\n").map((value) => value.trim()).filter(Boolean) } : { type: locateType, value: draft.locateValue }
    const actions = draft.actions.map((action) => action.op === "replace_text" ? { op: action.op, from: action.from || null, with: action.with } : { op: action.op, pattern: action.pattern, with: action.with })
    return { kind: draft.kind, phase: draft.phase, locate, actions, limit: draft.limit === "" ? null : Number(draft.limit) }
  }
  return { kind: "header", name: draft.headerName, value: draft.headerValue, mode: draft.headerMode }
}
