import { useTranslation } from "react-i18next"
import { ManagementDialog } from "@/components/management-dialog"
import { QuotasPanel } from "@/pages/quotas"
export function UserQuotasDialog({ userId, userName, onClose }: { userId: string; userName: string; onClose: () => void }) {
  const { t } = useTranslation()
  return <ManagementDialog title={t("userQuota.title", { name: userName })} onClose={onClose}><QuotasPanel ownerKind="user" ownerId={userId} /></ManagementDialog>
}
