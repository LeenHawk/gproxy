import { useTranslation } from "react-i18next"
import { InstantCell } from "@/components/cells"

export function CredentialExpiry({ value, pendingRefresh = false }: { value: number | null; pendingRefresh?: boolean }) {
  const { t } = useTranslation()
  return pendingRefresh && value === 0
    ? <span className="whitespace-nowrap text-xs">{t("management.pendingInitialRefresh")}</span>
    : <InstantCell value={value} />
}
