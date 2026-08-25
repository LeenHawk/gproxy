import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { CycleWindow } from "@/components/cycle-window"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

function latestCycles(cycles: Array<CredentialQuotaCycleDto>) {
  const byKey = new Map<string, CredentialQuotaCycleDto>()
  for (const cycle of cycles) {
    const current = byKey.get(cycle.window_key)
    if (!current || cycle.last_observed_at > current.last_observed_at) byKey.set(cycle.window_key, cycle)
  }
  return [...byKey.values()].sort((left, right) => left.window_key.localeCompare(right.window_key))
}

type Props = {
  cycles: Array<CredentialQuotaCycleDto>
  loading: boolean
  error: boolean
}

export function CredentialCycleList({ cycles, loading, error }: Props) {
  const { t } = useTranslation()
  const latest = useMemo(() => latestCycles(cycles), [cycles])

  if (loading) return <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
  if (error) return <p className="text-sm text-destructive">{t("common.errors.load")}</p>
  if (!latest.length) return <p className="text-sm text-muted-foreground">{t("providers.credentials.noQuotaCycle")}</p>
  return <div className="grid gap-3 lg:grid-cols-2">{latest.map((cycle) => <Card key={cycle.id} size="sm"><CardHeader><CardTitle headingLevel={4}>{t("providers.credentials.quotaCycle")}</CardTitle><CardDescription className="machine-text">{cycle.window_key}</CardDescription></CardHeader><CardContent><CycleWindow cycle={cycle} label={cycle.window_key} /></CardContent></Card>)}</div>
}
