import type { CredentialModelHealthDto } from "@/generated/CredentialModelHealthDto"
import { useTranslation } from "react-i18next"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { formatInstant } from "@/lib/format"

export function CredentialModelHealth({ values }: { values: Array<CredentialModelHealthDto> }) {
  const { t, i18n } = useTranslation()
  if (!values.length) return null
  return <section className="flex flex-col gap-2" aria-label={t("providers.credentials.modelHealth.title")}>
    <h4 className="text-sm font-medium">{t("providers.credentials.modelHealth.title")}</h4>
    <div className="grid gap-2 lg:grid-cols-2">
      {values.map((value) => <div key={value.model} className="rounded-lg border p-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <code className="text-xs">{value.model === "*" ? t("providers.credentials.modelHealth.credentialWide") : value.model}</code>
          <span className="flex items-center gap-2">
            {value.response_status == null ? null : <Badge variant="outline" className="font-mono">{value.response_status}</Badge>}
            <StatusBadge status={value.health} />
          </span>
        </div>
        <p className="mt-2 text-xs text-muted-foreground">{formatInstant(value.observed_at, i18n.language)}</p>
        {value.detail ? <p className="mt-1 font-mono text-xs text-muted-foreground">{value.detail}</p> : null}
      </div>)}
    </div>
  </section>
}
