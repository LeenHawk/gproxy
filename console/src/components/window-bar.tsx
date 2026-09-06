import { useTranslation } from "react-i18next"
import type { BoundaryConfidenceDto } from "@/generated/BoundaryConfidenceDto"
import type { BoundarySourceDto } from "@/generated/BoundarySourceDto"
import type { QuotaCoverageDto } from "@/generated/QuotaCoverageDto"
import { formatCost, formatInstant, formatNumber, formatPercent } from "@/lib/format"
import { cn } from "@/lib/utils"

export type WindowBarProps = {
  label: string
  used: string | number
  limit: string | number
  start?: number | null
  end?: number | null
  started?: boolean
  resetLabel?: string
  boundary?: BoundarySourceDto
  confidence?: BoundaryConfidenceDto
  coverage?: QuotaCoverageDto
  unit?: "cost" | "number" | "percent"
}

export function WindowBar(props: WindowBarProps) {
  const { t, i18n } = useTranslation()
  const used = Number(props.used)
  const limit = Number(props.limit)
  const ratio = limit > 0 ? Math.min(Math.max(used / limit, 0), 1) : 1
  const state = ratio >= 1 ? "critical" : ratio >= 0.85 ? "warning" : "healthy"
  const started = props.started ?? true
  const uncertain = props.boundary === "inferred" || props.boundary === "unknown"
  const coverage = props.coverage === "partial_lower_bound"
    ? t("window.coverage.partial")
    : props.coverage === "unknown" ? t("window.coverage.unknown") : null
  const reset = formatInstant(props.end ?? null, i18n.language)
  const start = formatInstant(props.start ?? null, i18n.language)
  const formatValue = (value: number) => {
    if (props.unit === "percent") return formatPercent(value / 100, i18n.language)
    if (props.unit === "number") return formatNumber(value, i18n.language)
    return formatCost(value, i18n.language)
  }

  return (
    <section className="flex flex-col gap-2" aria-label={props.label}>
      <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium">{props.label}</span>
          {props.boundary && props.boundary !== "upstream" ? <span className="text-xs text-muted-foreground">{t(`window.boundary.${props.boundary}`)}</span> : null}
          {props.confidence && props.confidence !== "exact" ? <span className="text-xs text-muted-foreground">{t(`window.confidence.${props.confidence}`)}</span> : null}
          {coverage ? <span className="text-xs text-muted-foreground">{coverage}</span> : null}
        </div>
        <span className="font-mono text-xs text-muted-foreground">
          {started ? `${formatValue(used)} / ${formatValue(limit)}` : t("window.notStarted")}
        </span>
      </div>
      <div
        className={cn("relative h-3 overflow-hidden rounded-sm bg-muted ring-1 ring-inset ring-border", uncertain && "border-r border-dashed")}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(ratio * 100)}
        aria-valuetext={started ? formatPercent(ratio, i18n.language) : t("window.notStarted")}
      >
        <div
          className={cn(
            "h-full transition-[width]",
            state === "healthy" && "bg-state-healthy",
            state === "warning" && "bg-state-warning",
            state === "critical" && "bg-state-critical",
            props.coverage === "partial_lower_bound" && "window-bar-partial",
          )}
          style={{ width: started ? `${ratio * 100}%` : "0%" }}
        />
      </div>
      <div className="flex justify-between gap-3 text-xs text-muted-foreground">
        <span>{started ? `${formatPercent(ratio, i18n.language)}${start ? ` · ${t("window.started", { time: start })}` : ""}` : t("window.notStarted")}</span>
        <span>{props.resetLabel ?? (reset ? t("window.resets", { value: reset }) : t("window.noReset"))}</span>
      </div>
    </section>
  )
}
