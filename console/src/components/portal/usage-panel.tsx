import { useTranslation } from "react-i18next"
import type { PortalUsageDto } from "@/generated/PortalUsageDto"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { QueryState } from "@/components/query-state"
import { Table, TableBody, TableCell, TableRow } from "@/components/ui/table"
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
          <Table>
            <TableBody>
              <TableRow><TableCell>{t("portal.usage.cost")}</TableCell><TableCell className="text-right font-mono">{formatCost(usage?.cost ?? "0", i18n.language)}</TableCell></TableRow>
              <TableRow><TableCell>{t("portal.usage.requests")}</TableCell><TableCell className="text-right font-mono">{formatCount(usage?.requests ?? 0, i18n.language)}</TableCell></TableRow>
              <TableRow><TableCell>{t("portal.usage.inputTokens")}</TableCell><TableCell className="text-right font-mono">{formatCount(usage?.input_tokens ?? 0, i18n.language)}</TableCell></TableRow>
              <TableRow><TableCell>{t("portal.usage.outputTokens")}</TableCell><TableCell className="text-right font-mono">{formatCount(usage?.output_tokens ?? 0, i18n.language)}</TableCell></TableRow>
              <TableRow><TableCell>{t("portal.usage.cachedTokens")}</TableCell><TableCell className="text-right font-mono">{formatCount(usage?.cached_input_tokens ?? 0, i18n.language)}</TableCell></TableRow>
            </TableBody>
          </Table>
        </QueryState>
      </CardContent>
    </Card>
  )
}
