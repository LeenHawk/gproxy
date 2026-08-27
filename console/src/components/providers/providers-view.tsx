import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import { PlusIcon } from "lucide-react"
import { useMemo } from "react"
import { useTranslation } from "react-i18next"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { ConnectivityTest } from "@/components/connectivity-test"
import { PageHeader } from "@/components/page-header"
import { ProviderDetail } from "@/components/providers/provider-detail"
import { ProviderDialog } from "@/components/providers/provider-dialog"
import type { RuleMutations } from "@/components/rules/rules-workspace"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"
import { cn } from "@/lib/utils"

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
  ruleMutations: RuleMutations
}

export function ProvidersView(props: Props) {
  const { t } = useTranslation()
  const location = useAdminLocation()
  const selectedId = Number(location.segments[0])
  const selected = props.providers.find((provider) => provider.id === selectedId) ?? null
  const tab = ["rules", "routing", "settings"].includes(location.segments[1]) ? location.segments[1] as "rules" | "routing" | "settings" : "credentials"
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
  const columns: Array<DataTableColumn<ProviderDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (provider) => <div><p className="font-medium">{provider.label ?? provider.name}</p>{provider.label ? <p className="font-mono text-xs text-muted-foreground">{provider.name}</p> : null}</div> },
    { key: "channel", label: t("providers.fields.channel"), header: t("providers.fields.channel"), cell: (provider) => <span className="font-mono text-xs">{provider.channel}</span> },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: (provider) => <Badge variant={provider.enabled ? "outline" : "secondary"}>{t(`common.status.${provider.enabled ? "enabled" : "disabled"}`)}</Badge> },
    { key: "connectivity", label: t("connectivity.action"), header: <span className="sr-only">{t("connectivity.action")}</span>, cell: (provider) => <div onClick={(event) => event.stopPropagation()}><ConnectivityTest request={{ scope: "provider", provider_id: provider.id, credential_id: null }} label={provider.label ?? provider.name} /></div>, className: "text-right" },
  ]

  return (
    <section className="flex flex-col gap-5">
      <PageHeader
        title={t("providers.title")}
        description={t("providers.subtitle")}
        actions={(
          <ProviderDialog
            channels={props.channels}
            channelsLoading={props.channelsLoading}
            channelsError={props.channelsError}
            presets={props.presets}
            presetsLoading={props.presetsLoading}
            presetsError={props.presetsError}
            onSave={props.onSaveProvider}
            trigger={<Button><PlusIcon data-icon="inline-start" />{t("providers.add")}</Button>}
          />
        )}
      />
      {props.providersLoading || props.providersError ? <p className="text-sm text-muted-foreground">{props.providersError ? t("providers.loadError") : t("common.loading")}</p> : (
        <div className="grid min-w-0 gap-5 md:grid-cols-[minmax(18rem,0.75fr)_minmax(0,1.25fr)]">
          <aside className={cn("min-w-0", selected && "hidden md:block")}>
            <DataTable
              columns={columns}
              rows={props.providers}
              rowKey={(provider) => provider.id}
              searchText={(provider) => `${provider.label ?? ""} ${provider.name} ${provider.channel}`}
              renderCard={(provider) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="truncate font-medium">{provider.label ?? provider.name}</p><p className="truncate font-mono text-xs text-muted-foreground">{provider.label ? `${provider.name} · ` : ""}{provider.channel}</p></div><div className="flex items-center gap-2" onClick={(event) => event.stopPropagation()}><Badge variant={provider.enabled ? "outline" : "secondary"}>{t(`common.status.${provider.enabled ? "enabled" : "disabled"}`)}</Badge><ConnectivityTest request={{ scope: "provider", provider_id: provider.id, credential_id: null }} label={provider.label ?? provider.name} /></div></div>}
              empty={t("providers.empty")}
              storageKey="providers"
              selectable
              batchActions={(rows) => <BatchActions entity="providers" rows={rows} queryKeys={["providers"]} />}
              activeRowKey={selected?.id}
              onRowClick={(provider) => navigateAdminPath(`/admin/providers/${provider.id}/credentials`)}
            />
          </aside>
          <section className={cn("min-w-0", !selected && "hidden md:block")}>
            {selected ? <>
              <Button className="mb-3 md:hidden" variant="ghost" onClick={() => navigateAdminPath(adminPath("providers"))}>{t("common.actions.back")}</Button>
              <ProviderDetail
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
                ruleMutations={props.ruleMutations}
              />
            </> : <div className="grid min-h-80 place-items-center text-sm text-muted-foreground">{t("providers.selectPrompt")}</div>}
          </section>
        </div>
      )}
    </section>
  )
}
