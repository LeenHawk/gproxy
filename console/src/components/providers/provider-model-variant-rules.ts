import { deleteRule, saveProviderRuleSet, saveRule, saveRuleSet } from "@/api/control"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { VariantAction } from "@/components/providers/variant-presets/types"

export type VariantRuleRow = {
  name: string
  actions: Array<VariantAction>
  touched: boolean
}

export type VariantRuleChanges = {
  oldNames: Array<string>
  rows: Array<VariantRuleRow>
}

const sentinel = (providerId: number) => `gproxy:provider-default:${providerId}`

function dedicatedSet(providerId: number, ruleSets: Array<RuleSetDto>, attachments: Array<ProviderRuleSetDto>) {
  const attached = new Set(attachments.filter((item) => item.provider_id === providerId).map((item) => item.rule_set_id))
  return ruleSets.find((set) => attached.has(set.id) && set.description === sentinel(providerId))
}

export function variantRuleActions(providerId: number, ruleSets: Array<RuleSetDto>, rules: Array<RuleDto>, attachments: Array<ProviderRuleSetDto>) {
  const set = dedicatedSet(providerId, ruleSets, attachments)
  const actions = new Map<string, Array<VariantAction>>()
  if (!set) return actions
  const ordered = [...rules.filter((rule) => rule.rule_set_id === set.id)].sort((left, right) => left.sort_order - right.sort_order || left.id - right.id)
  for (const rule of ordered) {
    if (!rule.enabled || !rule.filter_model_pattern || rule.config.kind !== "rewrite" || rule.config.action !== "set") continue
    const current = actions.get(rule.filter_model_pattern) ?? []
    current.push({ path: rule.config.path, value: rule.config.value })
    actions.set(rule.filter_model_pattern, current)
  }
  return actions
}

export async function syncVariantRules(provider: ProviderDto, changes: VariantRuleChanges, ruleSets: Array<RuleSetDto>, rules: Array<RuleDto>, attachments: Array<ProviderRuleSetDto>) {
  const rows = changes.rows.map((row) => ({ ...row, name: row.name.trim() })).filter((row) => row.name)
  const names = new Set(rows.map((row) => row.name))
  const removed = changes.oldNames.filter((name) => !names.has(name))
  const touched = rows.filter((row) => row.touched)
  if (removed.length === 0 && touched.length === 0) return

  let set = dedicatedSet(provider.id, ruleSets, attachments)
  if (!set) {
    const existing = ruleSets.find((item) => item.description === sentinel(provider.id))
    if (existing) {
      set = existing
    } else {
      const created = await saveRuleSet({ name: `${provider.name} · defaults`, description: sentinel(provider.id), enabled: true })
      if (!created) throw new Error("provider rule set was not created")
      set = { id: created.id, name: `${provider.name} · defaults`, description: sentinel(provider.id), enabled: true }
    }
  }
  const setId = set.id
  if (!attachments.some((item) => item.provider_id === provider.id && item.rule_set_id === setId)) {
    const order = attachments.filter((item) => item.provider_id === provider.id).reduce((value, item) => Math.max(value, item.sort_order + 1), 0)
    await saveProviderRuleSet({ provider_id: provider.id, rule_set_id: setId, sort_order: order, enabled: true })
  }

  const replaced = new Set([...removed, ...touched.map((row) => row.name)])
  const stale = rules.filter((rule) => rule.rule_set_id === setId && rule.filter_model_pattern && replaced.has(rule.filter_model_pattern))
  await Promise.all(stale.map((rule) => deleteRule(rule.id)))
  await Promise.all(touched.flatMap((row) => row.actions.map((action, sortOrder) => saveRule({
    rule_set_id: setId,
    config: { kind: "rewrite", path: action.path, action: "set", value: action.value },
    filter_model_pattern: row.name,
    filter_operations: null,
    filter_header_pattern: null,
    sort_order: sortOrder,
    enabled: true,
  }))))
}
