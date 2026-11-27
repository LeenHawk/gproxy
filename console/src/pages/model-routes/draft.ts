import type { BatchItem, RouteDto, RouteMemberDto, RouteMemberPatch, RouteMemberWrite, RoutePatch } from "@/generated/sdk"

export type Strategy = "failover" | "round_robin" | "weighted"
export const STRATEGIES: Strategy[] = ["failover", "weighted", "round_robin"]

/** A member while it is being edited; `id` is absent until it is saved. */
export type DraftMember = { key: string; id?: string; providerId: string; upstreamModel: string; tier: number; weight: number; enabled: boolean }
export type Draft = { name: string; strategy: Strategy; sessionAffinity: boolean; maxAttempts: number; enabled: boolean; members: DraftMember[] }

let counter = 0
export const newKey = () => `new-${++counter}`

export function draftOf(route: RouteDto | null, members: RouteMemberDto[], defaultAttempts: number): Draft {
  return {
    name: route?.name ?? "",
    strategy: (route?.strategy as Strategy | undefined) ?? "failover",
    sessionAffinity: route?.sessionAffinity ?? false,
    maxAttempts: route?.maxAttempts ?? defaultAttempts,
    enabled: route?.enabled ?? true,
    members: normalize([...members].sort((a, b) => a.tier - b.tier || b.weight - a.weight).map(member => ({ key: member.id, id: member.id, providerId: member.providerId, upstreamModel: member.upstreamModel, tier: member.tier, weight: member.weight, enabled: member.enabled }))),
  }
}

/** Members grouped by tier (stable within one), with tiers renumbered 0..n so the layers read as first, second, third. */
export function normalize(members: DraftMember[]): DraftMember[] {
  const tiers = [...new Set(members.map(member => member.tier))].sort((a, b) => a - b)
  return members
    .map(member => ({ ...member, tier: tiers.indexOf(member.tier) }))
    .sort((a, b) => a.tier - b.tier)
}

export function tiersOf(members: DraftMember[]): DraftMember[][] {
  const layers: DraftMember[][] = []
  for (const member of members) (layers[member.tier] ??= []).push(member)
  return layers.filter(Boolean)
}

/** The share of a tier's requests a member takes under weighted routing; null when it takes none. */
export function shareOf(member: DraftMember, layer: DraftMember[], usable: (member: DraftMember) => boolean): number | null {
  if (!usable(member)) return null
  const total = layer.filter(usable).reduce((sum, other) => sum + Math.max(other.weight, 1), 0)
  return total ? Math.round((100 * Math.max(member.weight, 1)) / total) : null
}

export function routePatch(route: RouteDto, draft: Draft): Partial<RoutePatch> | null {
  const patch: Partial<RoutePatch> = {}
  if (draft.name.trim() !== route.name) patch.name = draft.name.trim()
  if (draft.strategy !== route.strategy) patch.strategy = draft.strategy
  if (draft.sessionAffinity !== route.sessionAffinity) patch.sessionAffinity = draft.sessionAffinity
  if (draft.maxAttempts !== route.maxAttempts) patch.maxAttempts = draft.maxAttempts
  if (draft.enabled !== route.enabled) patch.enabled = draft.enabled
  return Object.keys(patch).length ? patch : null
}

export function memberBatch(routeId: string, original: RouteMemberDto[], draft: Draft): BatchItem<Partial<RouteMemberWrite>, Partial<RouteMemberPatch>>[] {
  const items: BatchItem<Partial<RouteMemberWrite>, Partial<RouteMemberPatch>>[] = []
  const kept = new Set(draft.members.flatMap(member => member.id ? [member.id] : []))
  for (const member of original) if (!kept.has(member.id)) items.push({ delete: member.id })
  for (const member of draft.members) {
    if (!member.id) {
      items.push({ create: { routeId, providerId: member.providerId, upstreamModel: member.upstreamModel, tier: member.tier, weight: member.weight, enabled: member.enabled } })
      continue
    }
    const before = original.find(row => row.id === member.id)
    if (!before) continue
    const patch: Partial<RouteMemberPatch> = {}
    if (member.tier !== before.tier) patch.tier = member.tier
    if (member.weight !== before.weight) patch.weight = member.weight
    if (member.enabled !== before.enabled) patch.enabled = member.enabled
    if (Object.keys(patch).length) items.push({ update: { id: member.id, patch } })
  }
  return items
}

export const sameDraft = (a: Draft, b: Draft) => JSON.stringify(a) === JSON.stringify(b)
