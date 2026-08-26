import { useTranslation } from "react-i18next"
import type { PortalUsageDto } from "@/generated/PortalUsageDto"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { QueryState } from "@/components/query-state"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { formatCost, formatCount } from "@/lib/format"

export type UsageDays = 1 | 7 | 30

const ranges: Array<UsageDays> = [1, 7, 30]

export function UsagePanel({
  usage,
  days,
  loading,
  error,
  onDaysChange,
}: {
  usage: PortalUsageDto | undefined
  days: UsageDays
  loading: boolean
  error: boolean
  onDaysChange: (days: UsageDays) => void
}) {
  const { t, i18n } = useTranslation()

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("portal.usage.title")}</CardTitle>
        <CardDescription>{t("portal.usage.description")}</CardDescription>
        <CardAction>
          <ToggleGroup
            type="single"
            variant="outline"
            size="sm"
            value={String(days)}
            aria-label={t("portal.usage.rangeLabel")}
            onValueChange={(value) => {
              const next = Number(value)
              if (value && ranges.includes(next as UsageDays)) onDaysChange(next as UsageDays)
            }}
          >
            {ranges.map((range) => (
              <ToggleGroupItem key={range} value={String(range)}>
                {t(`portal.usage.ranges.${range}`)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </CardAction>
      </CardHeader>
      <CardContent>
        <QueryState loading={loading} error={error ? t("portal.usage.loadError") : ""}>
          <dl className="grid gap-px overflow-hidden rounded-md border bg-border sm:grid-cols-2 lg:grid-cols-5">
            {[
              [t("portal.usage.cost"), formatCost(usage?.cost ?? "0", i18n.language)],
              [t("portal.usage.requests"), formatCount(usage?.requests ?? 0, i18n.language)],
              [t("portal.usage.inputTokens"), formatCount(usage?.input_tokens ?? 0, i18n.language)],
              [t("portal.usage.outputTokens"), formatCount(usage?.output_tokens ?? 0, i18n.language)],
              [t("portal.usage.cachedTokens"), formatCount(usage?.cached_input_tokens ?? 0, i18n.language)],
            ].map(([label, value]) => <div key={label} className="bg-card p-3"><dt className="text-xs text-muted-foreground">{label}</dt><dd className="mt-1 font-mono text-sm">{value}</dd></div>)}
          </dl>
        </QueryState>
      </CardContent>
    </Card>
  )
}
