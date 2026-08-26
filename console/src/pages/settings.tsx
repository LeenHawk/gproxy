import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { InstanceSettingsForm } from "@/components/settings/instance-settings-form"

export function SettingsPage() {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  return (
    <PageLayout title={t("settings.title")} description={t("settings.subtitle")}>
      <QueryState loading={query.isLoading} error={query.error ? t("settings.loadError") : ""}>
        {query.data ? <InstanceSettingsForm settings={query.data} /> : null}
      </QueryState>
    </PageLayout>
  )
}
