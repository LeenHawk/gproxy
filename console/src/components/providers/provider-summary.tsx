import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialHealthDto } from "@/generated/CredentialHealthDto"
import { useTranslation } from "react-i18next"
import { StatusBadge } from "@/components/status-badge"

const rank: Record<CredentialHealthDto, number> = {
  disabled: 0,
  healthy: 1,
  unknown: 2,
  degraded: 3,
  dead: 4,
}

function providerHealth(credentials: Array<CredentialDto>): CredentialHealthDto {
  let health: CredentialHealthDto = credentials.length ? "disabled" : "unknown"
  for (const credential of credentials) {
    if (!credential.enabled) continue
    const values = [credential.health, ...credential.model_health.map((model) => model.health)]
    for (const value of values) {
      if (rank[value] > rank[health]) health = value
    }
  }
  return health
}

export function ProviderSummary({ channel, credentials }: { channel: string; credentials: Array<CredentialDto> }) {
  const { t } = useTranslation()
  return <>
    <span className="truncate font-mono">{channel}</span>
    <span aria-hidden>·</span>
    <span className="shrink-0">{t("providers.summary.credentialCount", { count: credentials.length })}</span>
    <span aria-hidden>·</span>
    <span className="shrink-0"><StatusBadge status={providerHealth(credentials)} /></span>
  </>
}
