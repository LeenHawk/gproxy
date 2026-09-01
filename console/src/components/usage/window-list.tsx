import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { CycleWindow } from "@/components/cycle-window"
import { windowName } from "@/lib/quota-window"

export function WindowList({ cycles }: { cycles: Array<CredentialQuotaCycleDto> }) {
  const { t } = useTranslation()
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
        return <CycleWindow key={cycle.id} cycle={cycle} label={`${windowName(cycle.window_key, t, label)} · #${cycle.credential_id}`} />
      }) : <p className="text-sm text-muted-foreground">{t("usage.cycleStates.empty")}</p>}
    </section>
  )
}
