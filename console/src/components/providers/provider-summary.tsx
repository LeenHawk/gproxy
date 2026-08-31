import type { CredentialDto } from "@/generated/CredentialDto"
import { useTranslation } from "react-i18next"

export function ProviderSummary({ channel, credentials }: { channel: string; credentials: Array<CredentialDto> }) {
  const { t } = useTranslation()
  return <>
    <span className="truncate font-mono">{channel}</span>
    <span aria-hidden>·</span>
    <span className="shrink-0">{t("providers.summary.credentialCount", { count: credentials.length })}</span>
  </>
}
