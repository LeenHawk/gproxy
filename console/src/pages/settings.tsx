import { useIsMutating, useQuery } from "@tanstack/react-query"
import { useState } from "react"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { Button } from "@/components/ui/button"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { InstanceSettingsForm } from "@/components/settings/instance-settings-form"
import { INSTANCE_SETTINGS_FORM_ID, INSTANCE_SETTINGS_MUTATION_KEY, SETTINGS_SECTIONS, type SettingsSection } from "@/components/settings/instance-settings-state"
import { ConfigurationTransferCard } from "@/components/settings/configuration-transfer-card"
import { AutostartCard } from "@/components/settings/autostart-card"
import { PortalSettingsCard } from "@/components/settings/portal-settings-card"
import { OAuthClientsCard } from "@/components/settings/oauth-clients-card"

export function SettingsPage() {
  const { t } = useTranslation()
  const [section, setSection] = useState<SettingsSection>("runtime")
  const instanceSection = section === "runtime" || section === "network" || section === "logs"
  const query = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  const saving = useIsMutating({ mutationKey: INSTANCE_SETTINGS_MUTATION_KEY }) > 0
  return (
    <PageLayout
      title={t("settings.title")}
      description={t("settings.subtitle")}
      actions={instanceSection ? <Button type="submit" form={INSTANCE_SETTINGS_FORM_ID} disabled={!query.data || saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button> : undefined}
    >
      <div className="flex max-w-4xl flex-col gap-6">
        <div className="max-w-full overflow-x-auto">
          <ToggleGroup type="single" variant="outline" size="sm" spacing={0} value={section} aria-label={t("settings.sections.label")} onValueChange={(value) => { if (value) setSection(value as SettingsSection) }}>
            {SETTINGS_SECTIONS.map((value) => <ToggleGroupItem key={value} value={value}>{t(`settings.sections.${value}`)}</ToggleGroupItem>)}
          </ToggleGroup>
        </div>
        <div hidden={!instanceSection}>
          <QueryState loading={query.isLoading} error={query.error ? t("settings.loadError") : ""}>
            {query.data ? <InstanceSettingsForm settings={query.data} section={section} /> : null}
          </QueryState>
        </div>
        <div hidden={section !== "access"}>
          <div className="flex flex-col gap-8"><PortalSettingsCard /><OAuthClientsCard /></div>
        </div>
        <div hidden={section !== "maintenance"}>
          <div className="flex flex-col gap-8"><AutostartCard /><ConfigurationTransferCard /></div>
        </div>
      </div>
    </PageLayout>
  )
}
