import type { CredentialQuotaCycleModelDto } from "@/generated/CredentialQuotaCycleModelDto"
import { ChevronsUpDownIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { formatCost, formatCount } from "@/lib/format"

const tokenMetrics = ["input_tokens", "output_tokens", "cached_input_tokens"] as const

export function CredentialCycleModels({ values }: { values: Array<CredentialQuotaCycleModelDto> }) {
  const { t } = useTranslation()
  if (!values.length) return null
  return (
    <Collapsible className="mt-0.5 border-t pt-1">
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="h-7 px-1 text-xs text-muted-foreground">
          <ChevronsUpDownIcon className="size-3" aria-hidden />
          {t("providers.credentials.cycleModels.byModel", { count: values.length })}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="grid gap-1 border-t pt-1.5">
        {values.map((value) => <ModelRow key={value.model} value={value} />)}
      </CollapsibleContent>
    </Collapsible>
  )
}

function ModelRow({ value }: { value: CredentialQuotaCycleModelDto }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const tokens = metric(value.metrics, "total_tokens")
  const breakdown = [
    `${t("providers.credentials.cycleModels.requests")} ${formatCount(metric(value.metrics, "requests"), locale)}`,
    ...tokenMetrics
      .filter((key) => metric(value.metrics, key) > 0)
      .map((key) => `${t(`providers.credentials.cycleModels.${key}`)} ${formatCount(metric(value.metrics, key), locale)}`),
  ].join(" · ")
  return (
    <div className="grid gap-0.5 py-1 text-xs">
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <span className="break-all font-mono font-medium">{value.model || t("providers.credentials.cycleModels.unknownModel")}</span>
        <span className="tabular-nums">
          {t("providers.credentials.cycleModels.total", { tokens: formatCount(tokens, locale), cost: formatCost(metric(value.metrics, "cost"), locale) })}
        </span>
      </div>
      <span className="text-muted-foreground">{breakdown}</span>
    </div>
  )
}

function metric(metrics: unknown, key: string): number {
  if (typeof metrics !== "object" || metrics == null || Array.isArray(metrics)) return 0
  const value = (metrics as Record<string, unknown>)[key]
  const parsed = typeof value === "string" || typeof value === "number" ? Number(value) : 0
  return Number.isFinite(parsed) ? parsed : 0
}
