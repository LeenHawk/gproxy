import type { TFunction } from "i18next"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CycleObservationDto } from "@/generated/CycleObservationDto"
import type { ProviderDto } from "@/generated/ProviderDto"
import { windowName } from "@/lib/quota-window"

export type QuotaMetric = "percent" | "tokens" | "cost"
export type QuotaSeries = { id: string; providerId: string; provider: string; label: string; color: string; cycles: Array<CredentialQuotaCycleDto> }
export type QuotaPoint = { at: number; value: number | null }
export type QuotaRound = { at: number; value: number; range: [number, number]; minimum: number; maximum: number; count: number; cycleId: number; observedAt: number }

function amount(value: string | null | undefined): number | null {
  if (value == null || !value.trim()) return null
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null
}

export function remainingQuota(sample: CycleObservationDto, metric: QuotaMetric): number | null {
  const used = amount(sample.upstream_used)
  const limit = amount(sample.upstream_limit)
  const percent = amount(sample.used_percent) ?? (used != null && limit != null && limit > 0 ? used / limit * 100 : null)
  if (percent == null) return null
  const remaining = Math.max(0, 100 - percent)
  if (metric === "percent") return remaining
  if (sample.estimate?.reason != null) return null
  const total = amount(sample.estimate?.[metric])
  return total == null ? null : total * remaining / 100
}

export function cyclePoints(cycle: CredentialQuotaCycleDto, metric: QuotaMetric): Array<QuotaPoint> {
  return cycle.observations.map((sample) => ({ at: sample.observed_at_ms, value: remainingQuota(sample, metric) }))
    .sort((left, right) => left.at - right.at)
}

export function roundRange(cycle: CredentialQuotaCycleDto, metric: QuotaMetric): QuotaRound | null {
  const valid = cyclePoints(cycle, metric).filter((point): point is QuotaPoint & { value: number } => point.value != null)
  const latest = valid.at(-1)
  if (!latest) return null
  const minimum = valid.reduce((value, point) => Math.min(value, point.value), latest.value)
  const maximum = valid.reduce((value, point) => Math.max(value, point.value), latest.value)
  return { at: cycle.accounting_start_ms, value: latest.value, range: [latest.value - minimum, maximum - latest.value], minimum, maximum, count: valid.length, cycleId: cycle.id, observedAt: latest.at }
}

export function quotaSeries(cycles: Array<CredentialQuotaCycleDto>, credentials: Array<CredentialDto>, providers: Array<ProviderDto>, t: TFunction): Array<QuotaSeries> {
  const credentialById = new Map(credentials.map((credential) => [credential.id, credential]))
  const providerById = new Map(providers.map((provider) => [provider.id, provider]))
  const groups = new Map<string, Array<CredentialQuotaCycleDto>>()
  for (const cycle of cycles) {
    const key = `${cycle.credential_id}:${cycle.window_key}`
    const group = groups.get(key) ?? []
    group.push(cycle)
    groups.set(key, group)
  }
  return Array.from(groups).sort(([left], [right]) => left.localeCompare(right)).map(([id, cycles], index) => {
    const ordered = [...cycles].sort((left, right) => left.accounting_start_ms - right.accounting_start_ms || left.id - right.id)
    const latest = cycles.reduce((latest, cycle) => cycle.last_observed_at > latest.last_observed_at ? cycle : latest)
    const credential = credentialById.get(latest.credential_id)
    const provider = credential ? providerById.get(credential.provider_id) : undefined
    const providerName = provider?.label ?? provider?.name ?? (credential ? `#${credential.provider_id}` : t("usage.quotaHistory.unknownProvider"))
    return {
      id, providerId: credential ? String(credential.provider_id) : "unknown", provider: providerName,
      label: `${providerName} · ${credential?.label ?? `#${latest.credential_id}`} · ${windowName(latest.window_key, t, latest.label)}`,
      color: `var(--${["state-info", "state-healthy", "state-warning", "state-critical", "primary"][index % 5]})`, cycles: ordered,
    }
  })
}
