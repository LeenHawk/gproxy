import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UpdatePanel } from "@/components/update/update-panel"
import { UpdatePreferences } from "@/components/update/update-preferences"

export function UpdatePage() {
  const { t } = useTranslation()
  const settings = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  return (
    <PageLayout title={t("update.title")} description={t("update.subtitle")}>
      <div className="flex max-w-4xl flex-col gap-6">
        <QueryState loading={settings.isLoading} error={settings.error ? t("update.preferences.loadError") : ""}>
          {settings.data ? <UpdatePreferences settings={settings.data} /> : null}
        </QueryState>
        <UpdatePanel />
      </div>
    </PageLayout>
  )
}
