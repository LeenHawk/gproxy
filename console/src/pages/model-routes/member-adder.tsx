import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Plus } from "lucide-react"
import { discoverModels } from "@/api/models"
import { providers, providerModels } from "@/api/configuration"
import { optionSource } from "@/api/options"
import { SearchableSelect } from "@/components/searchable-select"
import { Button } from "@/components/ui/button"

/** Pick a provider, then one of its models (discovered live when the provider allows it), and add it. */
export function MemberAdder({ idPrefix, onAdd }: { idPrefix: string; onAdd: (providerId: string, model: string) => void }) {
  const { t } = useTranslation()
  const [providerId, setProviderId] = useState("")
  const [model, setModel] = useState("")
  const provider = useQuery({ queryKey: ["admin", "/providers", "detail", providerId], queryFn: () => providers.get(providerId), enabled: !!providerId })
  const refresh = (provider.data?.config as { auto_refresh_models?: boolean } | null)?.auto_refresh_models !== false
  const discovered = useQuery({
    queryKey: ["route-member-models", providerId, refresh],
    queryFn: () => discoverModels(providerId),
    enabled: !!provider.data && refresh,
    retry: false,
    refetchOnWindowFocus: false,
  })
  const candidates = refresh && !discovered.isError ? discovered.data : undefined
  const add = () => { onAdd(providerId, model.trim()); setModel("") }
  return <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
    <div className="sm:w-56"><SearchableSelect id={`${idPrefix}-provider`} label={t("fields.providerId")} value={providerId} onChange={id => { setProviderId(id); setModel("") }} source={optionSource(providers, row => ({ value: row.id, label: row.displayName ?? row.name }))} options={provider.data ? [{ value: provider.data.id, label: provider.data.displayName ?? provider.data.name }] : []} /></div>
    <div className="min-w-0 flex-1">
      <SearchableSelect id={`${idPrefix}-model`} label={t("fields.upstreamModel")} value={model} onChange={setModel} disabled={!providerId} allowCustom options={candidates?.map(row => ({ value: row.upstreamName, label: row.upstreamName }))} source={candidates ? undefined : providerId ? optionSource(providerModels, row => ({ value: row.upstreamName, label: row.upstreamName }), { providerId }) : undefined} />
      {providerId && refresh && discovered.isFetching ? <p className="mt-1 text-xs text-muted-foreground" role="status">{t("modelRoutes.modelsLoading")}</p> : null}
      {refresh && discovered.isError ? <p className="mt-1 text-xs text-muted-foreground" role="status">{t("modelRoutes.modelsFallback")}</p> : null}
    </div>
    <Button type="button" variant="outline" disabled={!providerId || !model.trim()} onClick={add}><Plus data-icon="inline-start" />{t("modelRoutes.addMember")}</Button>
  </div>
}
