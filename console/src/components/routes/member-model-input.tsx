import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { discoverModels, providerModels } from "@/api/control"
import type { ProviderDto } from "@/generated/ProviderDto"
import { ModelInput } from "@/components/model-input"
import { FieldDescription } from "@/components/ui/field"

export function MemberModelInput({ provider, id, value, onChange }: {
  provider?: ProviderDto
  id: string
  value: string
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  const settings = provider?.settings as { auto_refresh_models?: boolean } | null
  const refresh = settings?.auto_refresh_models !== false
  const stored = useQuery({ queryKey: ["provider-models"], queryFn: providerModels })
  const discovered = useQuery({
    queryKey: ["route-member-models", provider?.id, refresh],
    enabled: !!provider && refresh,
    queryFn: async () => {
      const result = await discoverModels({ provider_id: provider!.id })
      if (!result.ok) throw new Error("Model discovery failed")
      return result.models
    },
    retry: false,
    refetchOnWindowFocus: false,
    refetchOnMount: "always",
  })
  const models = refresh && discovered.data ? discovered.data : (stored.data ?? []).filter((model) => model.provider_id === provider?.id)
  return <>
    <ModelInput id={id} value={value} onChange={onChange} required options={[...new Set(models.map((model) => model.model_id))].sort()} />
    {discovered.isFetching ? <FieldDescription>{t("common.loading")}</FieldDescription> : null}
    {refresh && discovered.isError ? <FieldDescription role="status">{t("routes.members.modelsFallback")}</FieldDescription> : null}
  </>
}
