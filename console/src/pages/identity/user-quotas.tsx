import { useIsMutating } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ManagementDialog } from "@/components/management-dialog"
import { QuotasPanel } from "@/pages/quotas"
export function UserQuotasDialog({ userId, userName, onClose }: { userId: string; userName: string; onClose: () => void }) {
  const { t } = useTranslation()
  const busy = useIsMutating() > 0
  return <ManagementDialog className="sm:max-w-2xl" busy={busy} title={`${userName} · ${t("limits.budget")}`} onClose={onClose}><QuotasPanel ownerKind="user" ownerId={userId} /></ManagementDialog>
}
