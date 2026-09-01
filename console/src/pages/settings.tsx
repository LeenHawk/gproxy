import { useIsMutating, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { Button } from "@/components/ui/button"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { INSTANCE_SETTINGS_FORM_ID, INSTANCE_SETTINGS_MUTATION_KEY, InstanceSettingsForm } from "@/components/settings/instance-settings-form"
import { ConfigurationTransferCard } from "@/components/settings/configuration-transfer-card"
import { AutostartCard } from "@/components/settings/autostart-card"
import { UpdateCard } from "@/components/settings/update-card"

export function SettingsPage() {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const saving = useIsMutating({ mutationKey: INSTANCE_SETTINGS_MUTATION_KEY }) > 0
  return (
    <PageLayout
      title={t("settings.title")}
      description={t("settings.subtitle")}
      actions={<Button type="submit" form={INSTANCE_SETTINGS_FORM_ID} disabled={!query.data || saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button>}
    >
      <div className="flex max-w-4xl flex-col gap-8">
      <QueryState loading={query.isLoading} error={query.error ? t("settings.loadError") : ""}>
        {query.data ? <InstanceSettingsForm settings={query.data} /> : null}
      </QueryState>
      <ConfigurationTransferCard />
      <AutostartCard />
      <UpdateCard />
      </div>
    </PageLayout>
  )
}
