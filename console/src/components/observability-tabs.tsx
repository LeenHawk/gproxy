import { useTranslation } from "react-i18next"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { adminPath, navigateAdminPath } from "@/lib/admin-route"

export function ObservabilityTabs({ value }: { value: "usage" | "audit" | "logs" }) {
  const { t } = useTranslation()
  return (
    <Tabs value={value} onValueChange={(next) => navigateAdminPath(adminPath(next as typeof value))}>
      <TabsList className="max-w-full overflow-x-auto overflow-y-hidden">
        <TabsTrigger value="usage">{t("observability.tabs.usage")}</TabsTrigger>
        <TabsTrigger value="audit">{t("observability.tabs.audit")}</TabsTrigger>
        <TabsTrigger value="logs">{t("observability.tabs.logs")}</TabsTrigger>
      </TabsList>
    </Tabs>
  )
}
