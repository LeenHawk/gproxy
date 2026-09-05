import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { CycleWindow } from "@/components/cycle-window"
import { formatInstant } from "@/lib/format"
import { windowName } from "@/lib/quota-window"

export function WindowList({ cycles, labels }: { cycles: Array<CredentialQuotaCycleDto>; labels?: Map<number, string> }) {
  const { t, i18n } = useTranslation()
  const latestLabels = useMemo(() => {
    const values = new Map<string, { label: string; observedAt: number }>()
    for (const cycle of cycles) {
      if (!cycle.label) continue
      const key = `${cycle.credential_id}:${cycle.window_key}`
      const current = values.get(key)
      if (!current || cycle.last_observed_at > current.observedAt) {
        values.set(key, { label: cycle.label, observedAt: cycle.last_observed_at })
      }
    }
    return values
  }, [cycles])
  return (
    <section className="flex flex-col gap-5">
      <h2 className="text-sm font-semibold">{t("usage.credentialCycles")}</h2>
      {cycles.length ? cycles.map((cycle) => {
        const label = latestLabels.get(`${cycle.credential_id}:${cycle.window_key}`)?.label
        return (
          <div key={cycle.id} className="flex flex-col gap-3">
            <CycleWindow cycle={cycle} label={labels?.get(cycle.id) ?? `${windowName(cycle.window_key, t, label)} · #${cycle.credential_id}`} />
            <p className="text-xs text-muted-foreground">#{cycle.id} · {t("usage.cycleUsage.starts", { value: formatInstant(cycle.accounting_start_ms / 1000, i18n.language) })} · {t("usage.cycleUsage.observed", { value: formatInstant(cycle.last_observed_at, i18n.language) })}</p>
          </div>
        )
      }) : <p className="text-sm text-muted-foreground">{t("usage.cycleStates.empty")}</p>}
    </section>
  )
}
