import type { CredentialQuotaCycleModelDto } from "@/generated/CredentialQuotaCycleModelDto"
import { useTranslation } from "react-i18next"
import { formatCost, formatNumber } from "@/lib/format"

const metricNames = ["requests", "input_tokens", "output_tokens", "cached_input_tokens"] as const

export function CredentialCycleModels({ values }: { values: Array<CredentialQuotaCycleModelDto> }) {
  const { t, i18n } = useTranslation()
  if (!values.length) return null
  return <section className="mt-3 flex flex-col gap-2" aria-label={t("providers.credentials.cycleModels.title")}>
    <h5 className="text-xs font-medium">{t("providers.credentials.cycleModels.title")}</h5>
    {values.map((value) => <div key={value.model} className="rounded-lg border p-2.5">
      <code className="text-xs">{value.model || t("providers.credentials.cycleModels.unknownModel")}</code>
      <dl className="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-xs sm:grid-cols-3">
        {metricNames.map((metric) => <Metric key={metric} label={t(`providers.credentials.cycleModels.${metric}`)} value={metricValue(value.metrics, metric, i18n.language)} />)}
        <Metric label={t("providers.credentials.cycleModels.cost")} value={formatCost(rawMetric(value.metrics, "cost"), i18n.language)} />
      </dl>
    </div>)}
  </section>
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-muted-foreground">{label}</dt><dd className="font-mono">{value}</dd></div>
}

function metricValue(metrics: unknown, key: string, locale: string) {
  return formatNumber(Number(rawMetric(metrics, key)), locale)
}

function rawMetric(metrics: unknown, key: string) {
  if (typeof metrics !== "object" || metrics == null || Array.isArray(metrics)) return "0"
  const value = (metrics as Record<string, unknown>)[key]
  return typeof value === "string" || typeof value === "number" ? value : "0"
}
