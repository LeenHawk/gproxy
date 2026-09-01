import { useMemo } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { PriceRuleDto } from "@/generated/PriceRuleDto"
import type { ProviderModelDto } from "@/generated/ProviderModelDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { BatchActions } from "@/components/batch-actions"
import { ProviderDetail } from "@/components/providers/provider-detail"
import { ProviderDialog } from "@/components/providers/provider-dialog"
import { ProviderSummary } from "@/components/providers/provider-summary"
import type { RuleMutations } from "@/components/rules/rules-workspace"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

type Props = {
  providers: Array<ProviderDto>
  providersLoading: boolean
  providersError: boolean
  channels: Array<ChannelDto>
  channelsLoading: boolean
  channelsError: boolean
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  credentials: Array<CredentialDto>
  credentialsLoading: boolean
  credentialsError: boolean
  cycles: Array<CredentialQuotaCycleDto>
  cyclesLoading: boolean
  cyclesError: boolean
  savingProviderId: number | null
  savingCredentialId: number | null
  onSaveProvider: (value: ProviderWriteRequest, id?: number) => Promise<void>
  onSaveCredential: (value: CredentialWriteRequest, id?: number) => Promise<void>
  ruleSets: Array<RuleSetDto>
  rules: Array<RuleDto>
  attachments: Array<ProviderRuleSetDto>
  routingRules: Array<RoutingRuleDto>
  providerModels: Array<ProviderModelDto>
  priceRules: Array<PriceRuleDto>
  ruleMutations: RuleMutations
}

export function ProvidersView(props: Props) {
  const { t } = useTranslation()
  const location = useAdminLocation()
  const selectedId = Number(location.segments[0])
  const selected = props.providers.find((provider) => provider.id === selectedId) ?? null
  const tab = ["models", "rules", "routing", "settings"].includes(location.segments[1]) ? location.segments[1] as "models" | "rules" | "routing" | "settings" : "credentials"
  const activeCredentialId = tab === "credentials" ? Number(location.segments[2]) : Number.NaN
  const credentialsByProvider = useMemo(() => {
    const groups = new Map<number, Array<CredentialDto>>()
    for (const credential of props.credentials) {
      const group = groups.get(credential.provider_id) ?? []
      group.push(credential)
      groups.set(credential.provider_id, group)
    }
    return groups
  }, [props.credentials])
  const cyclesByCredential = useMemo(() => {
    const groups = new Map<number, Array<CredentialQuotaCycleDto>>()
    for (const cycle of props.cycles) {
      const group = groups.get(cycle.credential_id) ?? []
      group.push(cycle)
      groups.set(cycle.credential_id, group)
    }
    return groups
  }, [props.cycles])

  const setEnabled = async (provider: ProviderDto, enabled: boolean) => {
    try {
      await props.onSaveProvider({
        name: provider.name,
        label: provider.label,
        channel: provider.channel,
        settings: provider.settings,
        traffic_policy: provider.traffic_policy,
        credential_strategy: provider.credential_strategy,
        proxy_url: provider.proxy_url,
        tls_fingerprint: provider.tls_fingerprint,
        enabled,
      }, provider.id)
      toast.success(t("providers.form.updated"))
    } catch {
      toast.error(t("providers.form.updateError"))
    }
  }

  if (props.providersLoading || props.providersError) {
    return <p className="text-sm text-muted-foreground">{props.providersError ? t("providers.loadError") : t("common.loading")}</p>
  }

  return (
    <WorkspaceLayout
      storageKey="gproxy.workspace.providers.width"
      title={t("providers.title")}
      items={props.providers}
      selectedId={selected?.id ?? null}
      getSearchText={(provider) => `${provider.label ?? ""} ${provider.name} ${provider.channel}`}
      renderTitle={(provider) => provider.label ?? provider.name}
      renderSummary={(provider) => <ProviderSummary channel={provider.channel} credentials={credentialsByProvider.get(provider.id) ?? []} />}
      renderAction={(provider) => <Switch size="sm" checked={provider.enabled} onCheckedChange={(value) => void setEnabled(provider, value)} disabled={props.savingProviderId === provider.id || provider.invalid_tls_fingerprint != null || provider.tls_fingerprint_error != null} aria-label={`${t("providers.fields.enabled")}: ${provider.label ?? provider.name}`} />}
      onSelect={(provider) => navigateAdminPath(`/admin/providers/${provider.id}/credentials`)}
      onBack={() => navigateAdminPath(adminPath("providers"))}
      searchPlaceholder={t("providers.workspace.search")}
      emptyLabel={t("providers.empty")}
      resizeLabel={t("providers.workspace.resize")}
      selectAllLabel={t("common.dataTable.selectAll")}
      selectRowLabel={(provider) => `${t("common.dataTable.selectRow")}: ${provider.label ?? provider.name}`}
      selectedLabel={(count) => t("common.dataTable.selected", { count })}
      mobileBackLabel={t("providers.title")}
      createAction={<ProviderDialog channels={props.channels} channelsLoading={props.channelsLoading} channelsError={props.channelsError} presets={props.presets} presetsLoading={props.presetsLoading} presetsError={props.presetsError} onSave={props.onSaveProvider} trigger={<Button size="icon-sm" aria-label={t("providers.add")}><PlusIcon aria-hidden /></Button>} />}
      batchActions={(rows, onApplied) => <BatchActions entity="providers" rows={rows} queryKeys={["providers"]} onApplied={onApplied} size="xs" />}
      emptyState={<Empty className="min-h-[28rem]"><EmptyHeader><EmptyTitle>{t("providers.title")}</EmptyTitle><EmptyDescription>{t("providers.selectPrompt")}</EmptyDescription></EmptyHeader></Empty>}
    >
      {selected ? <ProviderDetail
        provider={selected}
        providers={props.providers}
        tab={tab}
        onTab={(value) => navigateAdminPath(`/admin/providers/${selected.id}/${value}`, true)}
        channel={props.channels.find((channel) => channel.id === selected.channel)}
        channels={props.channels}
        presets={props.presets}
        credentials={credentialsByProvider.get(selected.id) ?? []}
        cyclesByCredential={cyclesByCredential}
        credentialsLoading={props.credentialsLoading}
        credentialsError={props.credentialsError}
        cyclesLoading={props.cyclesLoading}
        cyclesError={props.cyclesError}
        savingProviderId={props.savingProviderId}
        savingCredentialId={props.savingCredentialId}
        onSaveProvider={props.onSaveProvider}
        onSaveCredential={props.onSaveCredential}
        activeCredentialId={Number.isFinite(activeCredentialId) ? activeCredentialId : null}
        onCredentialOpen={(credential) => navigateAdminPath(`/admin/providers/${selected.id}/credentials/${credential.id}`)}
        onCredentialClose={() => navigateAdminPath(`/admin/providers/${selected.id}/credentials`)}
        ruleSets={props.ruleSets}
        rules={props.rules}
        attachments={props.attachments}
        routingRules={props.routingRules}
        providerModels={props.providerModels}
        priceRules={props.priceRules}
        ruleMutations={props.ruleMutations}
      /> : null}
    </WorkspaceLayout>
  )
}
