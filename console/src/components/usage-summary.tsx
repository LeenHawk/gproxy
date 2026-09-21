//! Usage totals, as a row of figures.
//!
//! `truncated` is shown rather than hidden: the scan has a row cap, and a
//! summary that silently describes the first N records instead of all of them
//! is a number somebody will make a decision on.

import { useTranslation } from "react-i18next"
import type { UsageSummaryDto } from "@/generated/app"
import { formatCost, formatCount } from "@/lib/format"

export function UsageSummary({ summary }: { summary: UsageSummaryDto }) {
  const { t, i18n } = useTranslation()
  const figures: Array<[string, string]> = [
    ["requests", formatCount(summary.requests, i18n.language)],
    ["cost", formatCost(summary.cost, i18n.language)],
    ["inputTokens", formatCount(summary.inputTokens, i18n.language)],
    ["outputTokens", formatCount(summary.outputTokens, i18n.language)],
    ["cachedInputTokens", formatCount(summary.cachedInputTokens, i18n.language)],
    ["reasoningTokens", formatCount(summary.reasoningTokens, i18n.language)],
  ]
  return (
    <div className="space-y-2">
      <dl className="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
        {figures.map(([key, value]) => (
          <div key={key} className="rounded-lg border border-border p-3">
            <dt className="text-xs text-muted-foreground">{t(`fields.${key}`)}</dt>
            <dd className="mt-1 text-lg font-medium tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      {summary.truncated ? (
        <p className="text-xs text-state-warning">
          {t("usage.truncated", { scanned: formatCount(summary.scanned, i18n.language) })}
        </p>
      ) : null}
    </div>
  )
}
