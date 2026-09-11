import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { instanceSettings } from "@/api/control"
import { PageLayout } from "@/components/page-layout"
import { QueryState } from "@/components/query-state"
import { UpdatePanel } from "@/components/update/update-panel"
import { UpdatePreferences } from "@/components/update/update-preferences"
import { StoreUpdatePanel } from "@/components/update/store-update-panel"
import { buildIdentity } from "@/lib/build-info"

export function UpdatePage() {
  const { t } = useTranslation()
  const store = buildIdentity().kind === "microsoft-store"
  return (
    <PageLayout title={t("update.title")} description={t(store ? "update.store.subtitle" : "update.subtitle")}>
      <div className="flex max-w-4xl flex-col gap-6">
        {store ? <StoreUpdatePanel /> : <NativeUpdate />}
      </div>
    </PageLayout>
  )
}

function NativeUpdate() {
  const { t } = useTranslation()
  const settings = useQuery({ queryKey: ["instance-settings"], queryFn: instanceSettings })
  return <>
    <QueryState loading={settings.isLoading} error={settings.error ? t("update.preferences.loadError") : ""}>
      {settings.data ? <UpdatePreferences settings={settings.data} /> : null}
    </QueryState>
    <UpdatePanel />
  </>
}
