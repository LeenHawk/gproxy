import { VARIANT_PROTOCOL_LABELS, defaultVariantProtocol, gatewayActionPath, variantGroups } from "@/components/providers/variant-presets"
import type { VariantAction, VariantProtocol } from "@/components/providers/variant-presets/types"

export type VariantSelection = {
  protocol: VariantProtocol
  picks: Record<string, string>
  upstream: string
  preserved: Array<VariantAction>
}

function equal(left: unknown, right: unknown): boolean {
  if (Object.is(left, right)) return true
  if (Array.isArray(left) || Array.isArray(right)) return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.every((value, index) => equal(value, right[index]))
  if (!left || !right || typeof left !== "object" || typeof right !== "object") return false
  const a = left as Record<string, unknown>
  const b = right as Record<string, unknown>
  const keys = Object.keys(a)
  return keys.length === Object.keys(b).length && keys.every((key) => Object.hasOwn(b, key) && equal(a[key], b[key]))
}

function match(actual: Array<VariantAction>, expected: Array<VariantAction>, used: Set<number>): Array<number> | null {
  const matched: Array<number> = []
  for (const action of expected) {
    const index = actual.findIndex((candidate, item) => !used.has(item) && !matched.includes(item) && candidate.path === action.path && equal(candidate.value, action.value))
    if (index < 0) return null
    matched.push(index)
  }
  return matched
}

function select(protocol: VariantProtocol, channel: string, actions: Array<VariantAction>) {
  const picks: Record<string, string> = {}
  const used = new Set<number>()
  for (const group of variantGroups(protocol, channel)) {
    const index = group.entries.findIndex((entry) => {
      const matched = match(actions, entry.actions, used)
      if (!matched) return false
      matched.forEach((item) => used.add(item))
      return true
    })
    if (index >= 0) picks[group.key] = String(index)
  }
  let upstream = ""
  const path = gatewayActionPath(channel)
  if (path) {
    const index = actions.findIndex((action, item) => !used.has(item) && action.path === path && Array.isArray(action.value) && action.value.every((value) => typeof value === "string"))
    if (index >= 0) {
      upstream = (actions[index].value as Array<string>).join(", ")
      used.add(index)
    }
  }
  return { protocol, picks, upstream, preserved: actions.filter((_, index) => !used.has(index)), matched: used.size }
}

export function inferVariantSelection(channel: string, actions: Array<VariantAction>): VariantSelection {
  const fallback = defaultVariantProtocol(channel)
  const protocols = [fallback, ...(Object.keys(VARIANT_PROTOCOL_LABELS) as Array<VariantProtocol>).filter((protocol) => protocol !== fallback)]
  let best = select(protocols[0], channel, actions)
  for (const protocol of protocols.slice(1)) {
    const candidate = select(protocol, channel, actions)
    if (candidate.matched > best.matched) best = candidate
  }
  return best
}
