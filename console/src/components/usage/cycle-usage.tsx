import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { formatCost, formatCount } from "@/lib/format"

function amount(value: unknown): number | null {
  if (typeof value !== "number" && (typeof value !== "string" || !value.trim())) return null
  const parsed = Number(value)
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null
}

function metric(metrics: unknown, key: string): number | null {
  if (typeof metrics !== "object" || metrics == null || Array.isArray(metrics)) return null
  return amount((metrics as Record<string, unknown>)[key])
}

export function CycleUsage({ cycle }: { cycle: CredentialQuotaCycleDto }) {
  const { t, i18n } = useTranslation()
  const input = metric(cycle.metrics, "input_tokens")
  const output = metric(cycle.metrics, "output_tokens")
  const tokens = input != null && output != null
    ? input + output + ["cache_creation_5m_tokens", "cache_creation_30m_tokens", "cache_creation_1h_tokens"]
      .reduce((sum, key) => sum + (metric(cycle.metrics, key) ?? 0), 0)
    : null
  const cost = metric(cycle.metrics, "cost")
  const requests = metric(cycle.metrics, "requests")
  const upstreamUsed = amount(cycle.upstream_used)
  const upstreamLimit = amount(cycle.upstream_limit)
  const percent = amount(cycle.used_percent) ?? (upstreamUsed != null && upstreamLimit != null && upstreamLimit > 0
    ? upstreamUsed / upstreamLimit * 100 : null)
  const estimate = (used: number | null) => {
    if (used == null || used <= 0 || percent == null || percent <= 0) return null
    const total = used / percent * 100
    return Number.isFinite(total) ? total : null
  }
  const formatUsage = (tokens: number | null, cost: number | null) => [
    tokens != null ? t("usage.cycleUsage.tokens", { value: formatCount(Math.round(tokens), i18n.language) }) : null,
    cost != null ? formatCost(cost, i18n.language) : null,
  ].filter(Boolean).join(" · ")
  const used = [
    formatUsage(tokens, cost),
    requests != null ? t("usage.cycleUsage.requests", { value: formatCount(requests, i18n.language) }) : null,
  ].filter(Boolean).join(" · ")
  const estimated = formatUsage(estimate(tokens), estimate(cost))

  return (
    <div className="flex flex-col gap-1.5 text-xs">
      <dl className="grid gap-1.5 sm:grid-cols-2">
        <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
          <dt className="text-muted-foreground">{t("usage.cycleUsage.used")}</dt>
          <dd className="tabular-nums">{used || t("usage.cycleUsage.unknown")}</dd>
        </div>
        <div className="flex flex-wrap justify-between gap-x-3 gap-y-1">
          <dt className="text-muted-foreground">{t("usage.cycleUsage.estimated")}</dt>
          <dd className="tabular-nums">{estimated ? `≈ ${estimated}` : t("usage.cycleUsage.unavailable")}</dd>
        </div>
      </dl>
      <p className="text-muted-foreground">{t("usage.cycleUsage.hint")}</p>
    </div>
  )
}
