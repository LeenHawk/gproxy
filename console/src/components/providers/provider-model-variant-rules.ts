import { bindings, rules, ruleSets } from "@/api/routing-rules"
import { directory } from "@/api/models"
import { ApiError, api, json } from "@/api/client"
import type { RewriteRuleDto } from "@/generated/sdk"
import type { VariantAction } from "./variant-presets"
export type VariantRuleRow = { name: string; actions: VariantAction[]; touched: boolean }
export const variantSetId = (id: string) => `model-variants-${id}`
export async function variantRules(id: string): Promise<RewriteRuleDto[]> { return directory(rules, { ruleSetId: variantSetId(id) }) }
export function readVariants(metadata: Record<string, unknown>, records: RewriteRuleDto[]): VariantRuleRow[] {
  const names = Array.isArray(metadata.variants) ? metadata.variants.filter((v): v is string => typeof v === "string") : []
  return names.map(name => ({ name, touched: false, actions: records.filter(r => r.action === "set" && r.enabled && r.filterModelPattern === name).sort((a,b) => a.sortOrder - b.sortOrder).flatMap(r => { try { return (r.paths ?? []).map(path => ({ path, value: JSON.parse(r.replacement) as unknown })) } catch { return [] } }) }))
}
export async function saveVariantRules(providerId: string, providerName: string, modelId: string, modelName: string, variants: VariantRuleRow[]) {
  const id = variantSetId(modelId)
  const [set, attached] = await Promise.all([
    ruleSets.get(id).catch(error => { if (error instanceof ApiError && error.status === 404) return null; throw error }),
    bindings.list({ providerId, ruleSetId: id, pageSize: 1 }),
  ])
  if (!set) {
    if (!variants.length) return
    await ruleSets.create({ id, name: `${providerName} / ${modelName}`, description: `gproxy:model-variants:${modelId}`, enabled: true })
  }
  if (!attached.total) await bindings.create({ providerId, ruleSetId: id, enabled: true, sortOrder: 0 })
  const previous = await variantRules(modelId)
  // Keep manually managed rules and untouched variant rules, including their
  // operation/event filters. Presets only replace behavior the user edited.
  const names = new Set(variants.map(v => v.name.trim()))
  const touched = new Set(variants.filter(v => v.touched).map(v => v.name.trim()))
  const retained = previous.filter(r => r.filterModelPattern === null || (names.has(r.filterModelPattern) && !touched.has(r.filterModelPattern)))
  const writes = variants.filter(v => v.touched).flatMap(v => v.actions.map((a, i) => ({ phase: "request", action: "set", target: "body", paths: [a.path], pattern: ".*", replacement: JSON.stringify(a.value), filterModelPattern: v.name.trim(), filterEventPattern: "^response\\.create$", filterOperationKeys: ["generate_content", "stream_generate_content"].flatMap(operation => ["openai", "openai_chat", "claude", "gemini", "openai_responses_websocket"].map(dialect => ({ operation, dialect }))), sortOrder: i, enabled: true })))
  await api(`/admin/api/rule-sets/${encodeURIComponent(id)}/rules`, json("PUT", [...retained, ...writes]))
}
