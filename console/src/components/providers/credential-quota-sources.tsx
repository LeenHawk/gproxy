import type { QuotaSnapshot } from "@/generated/QuotaSnapshot"
import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { CredentialQuotaEntry } from "@/components/providers/credential-quota-entry"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { formatInstant } from "@/lib/format"

export function CredentialQuotaSources({ snapshot, refreshing }: { snapshot: QuotaSnapshot; refreshing: boolean }) {
  const { t, i18n } = useTranslation()
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(timer)
  }, [])
  if (!snapshot.sources.length) return <Alert><AlertDescription>{t("upstreamQuota.support.unsupported")}</AlertDescription></Alert>
  return <div className="flex flex-col gap-4">
    {snapshot.sources.map((source) => {
      const { capability } = source
      const entries = snapshot.entries.filter((entry) => entry.source_id === capability.id)
      const label = t(`upstreamQuota.sources.${capability.label}`, { defaultValue: capability.label })
      const stale = source.observed_at_ms != null && now - source.observed_at_ms >= 600_000
      return <section key={capability.id} aria-label={label} className="flex min-w-0 flex-col gap-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 className="text-sm font-medium">{label}</h3>
          <div className="flex flex-wrap gap-2">
            {stale ? <Badge variant="secondary">{t("upstreamQuota.stale")}</Badge> : null}
            <Badge variant="outline">{capability.support === "ready" && capability.mode === "response" ? t("upstreamQuota.response") : t(`upstreamQuota.support.${capability.support}`)}</Badge>
          </div>
        </div>
        {capability.reason ? <p className="text-sm text-muted-foreground">{capability.reason}</p> : null}
        {source.error ? <Alert variant="destructive"><AlertTitle>{t(entries.length ? "upstreamQuota.staleError" : "providers.credentials.quotaProbe.error")}</AlertTitle><AlertDescription>{source.error.message}</AlertDescription></Alert> : null}
        {source.observed_at_ms != null ? <p className="text-xs text-muted-foreground">{t("upstreamQuota.observed", { time: formatInstant(source.observed_at_ms / 1000, i18n.language) })}</p> : null}
        {!entries.length && capability.support === "ready" && !source.error && !refreshing ? <p className="text-sm text-muted-foreground">{t(capability.mode === "response" ? "upstreamQuota.notObserved" : source.observed_at_ms == null && !capability.automatic ? "upstreamQuota.manualQuery" : "upstreamQuota.empty")}</p> : null}
        <div className="grid min-w-0 gap-3 md:grid-cols-2">
          {entries.filter((entry) => entry.value.kind !== "window").map((entry) => <CredentialQuotaEntry key={entry.id} entry={entry} />)}
        </div>
      </section>
    })}
  </div>
}
