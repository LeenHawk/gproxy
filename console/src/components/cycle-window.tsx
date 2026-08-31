import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { windowName } from "@/lib/quota-window"
import { WindowBar } from "@/components/window-bar"

export function CycleWindow({ cycle, label: labelOverride }: { cycle: CredentialQuotaCycleDto; label?: string }) {
  const { t } = useTranslation()
  const hasUpstreamPair = cycle.upstream_used != null && cycle.upstream_limit != null
  const hasPercent = cycle.used_percent != null
  const status = t(`common.status.${cycle.status}`)
  const reason = cycle.close_reason ? t(`window.closeReason.${cycle.close_reason}`) : null
  const label = `${labelOverride ?? `${windowName(cycle.window_key, t)} · #${cycle.credential_id}`} · ${status}${reason ? ` · ${reason}` : ""}`
  if (!hasUpstreamPair && !hasPercent) {
    return <p className="text-sm text-muted-foreground"><span className="font-mono">{label}</span> — {t("window.usageUnknown")}</p>
  }
  return (
    <WindowBar
      label={label}
      used={hasUpstreamPair ? cycle.upstream_used! : cycle.used_percent!}
      limit={hasUpstreamPair ? cycle.upstream_limit! : "100"}
      start={cycle.period_start}
      end={cycle.period_end}
      boundary={cycle.boundary_source}
      confidence={cycle.boundary_confidence}
      coverage={cycle.coverage}
      unit={hasUpstreamPair ? "number" : "percent"}
    />
  )
}
