import { CableIcon, PencilIcon } from "lucide-react"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderWriteRequest } from "@/generated/ProviderWriteRequest"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { RoutingRuleDto } from "@/generated/RoutingRuleDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { ConnectivityTest } from "@/components/connectivity-test"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { CredentialList } from "@/components/providers/credential-list"
import { CredentialCard } from "@/components/providers/credential-card"
import { ProviderDialog } from "@/components/providers/provider-dialog"
import { ProviderRoutingRules } from "@/components/rules/provider-routing-rules"
import { RulesWorkspace, type RuleMutations } from "@/components/rules/rules-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

type Props = {
  provider: ProviderDto
  providers: Array<ProviderDto>
  tab: "credentials" | "rules" | "routing" | "settings"
  onTab: (tab: Props["tab"]) => void
  channel?: ChannelDto
  channels: Array<ChannelDto>
  presets: Array<TlsPresetDto>
  credentials: Array<CredentialDto>
  cyclesByCredential: Map<number, Array<CredentialQuotaCycleDto>>
  credentialsLoading: boolean
  credentialsError: boolean
  cyclesLoading: boolean
  cyclesError: boolean
  savingProviderId: number | null
  savingCredentialId: number | null
  onSaveProvider: (value: ProviderWriteRequest, id?: number) => Promise<void>
  onSaveCredential: (value: CredentialWriteRequest, id?: number) => Promise<void>
  activeCredentialId?: number | null
  onCredentialOpen?: (credential: CredentialDto) => void
  onCredentialClose?: () => void
  ruleSets: Array<RuleSetDto>
  rules: Array<RuleDto>
  attachments: Array<ProviderRuleSetDto>
  routingRules: Array<RoutingRuleDto>
  ruleMutations: RuleMutations
}

export function ProviderDetail(props: Props) {
  const { t } = useTranslation()
  const switchId = useId()
  const invalidFingerprint = props.provider.invalid_tls_fingerprint != null || props.provider.tls_fingerprint_error != null
  const activeCredential = props.credentials.find((credential) => credential.id === props.activeCredentialId)
  const setEnabled = async (enabled: boolean) => {
    try {
      await props.onSaveProvider({
        name: props.provider.name,
        label: props.provider.label,
        channel: props.provider.channel,
        settings: props.provider.settings,
        credential_strategy: props.provider.credential_strategy,
        proxy_url: props.provider.proxy_url,
        tls_fingerprint: props.provider.tls_fingerprint,
        enabled,
      }, props.provider.id)
      toast.success(t("providers.form.updated"))
    } catch {
      toast.error(t("providers.form.updateError"))
    }
  }
  const edit = (
    <ProviderDialog
      provider={props.provider}
      channels={props.channels}
      channelsLoading={false}
      channelsError={false}
      presets={props.presets}
      presetsLoading={false}
      presetsError={false}
      onSave={props.onSaveProvider}
      trigger={<Button variant="outline" size="sm"><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>}
    />
  )
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex flex-wrap items-center justify-between gap-3 border-b pb-4">
        <div className="min-w-0">
          <h2 className="truncate text-xl font-semibold">{props.provider.label ?? props.provider.name}</h2>
          <p className="flex items-center gap-2 text-xs text-muted-foreground"><CableIcon aria-hidden />{props.provider.name} · {props.channel?.display_name ?? props.provider.channel}</p>
        </div>
        <div className="flex items-center gap-2">
          <ConnectivityTest request={{ scope: "provider", provider_id: props.provider.id, credential_id: null, proxy_url: null }} label={props.provider.label ?? props.provider.name} />
          <Badge variant={props.provider.enabled ? "outline" : "secondary"}>{t(`common.status.${props.provider.enabled ? "enabled" : "disabled"}`)}</Badge>
          <Field orientation="horizontal" className="w-auto">
            <FieldLabel htmlFor={switchId} className="sr-only">{t("providers.fields.enabled")}</FieldLabel>
            <Switch id={switchId} size="sm" checked={props.provider.enabled} onCheckedChange={(value) => void setEnabled(value)} disabled={props.savingProviderId === props.provider.id || invalidFingerprint} />
          </Field>
          {edit}
          <EntityDeleteButton entity="providers" id={props.provider.id} label={props.provider.label ?? props.provider.name} queryKeys={["providers", "credentials"]} />
        </div>
      </header>
      {invalidFingerprint ? <Alert variant="destructive"><AlertTitle>{t("providers.fingerprint.title")}</AlertTitle><AlertDescription>{props.provider.tls_fingerprint_error ?? t("providers.fingerprint.invalid")}</AlertDescription></Alert> : null}
      <Tabs value={props.tab} onValueChange={(value) => props.onTab(value as Props["tab"])}>
        <TabsList>
          <TabsTrigger value="credentials">{t("providers.credentials.title")}</TabsTrigger>
          <TabsTrigger value="rules">{t("rules.providerTab")}</TabsTrigger>
          <TabsTrigger value="routing">{t("rules.routing.tab")}</TabsTrigger>
          <TabsTrigger value="settings">{t("providers.tabs.settings")}</TabsTrigger>
        </TabsList>
        <TabsContent value="credentials" className="pt-4">
          {activeCredential ? <div className="flex flex-col gap-3">
            <Button className="self-start" variant="ghost" onClick={props.onCredentialClose}>{t("common.actions.back")}</Button>
            <CredentialCard
              credential={activeCredential}
              channel={props.channel}
              presets={props.presets}
              cycles={props.cyclesByCredential.get(activeCredential.id) ?? []}
              cyclesLoading={props.cyclesLoading}
              cyclesError={props.cyclesError}
              saving={props.savingCredentialId === activeCredential.id}
              onSave={props.onSaveCredential}
            />
          </div> : <CredentialList
            providerId={props.provider.id}
            channel={props.channel}
            presets={props.presets}
            credentials={props.credentials}
            cyclesByCredential={props.cyclesByCredential}
            credentialsLoading={props.credentialsLoading}
            credentialsError={props.credentialsError}
            cyclesLoading={props.cyclesLoading}
            cyclesError={props.cyclesError}
            savingCredentialId={props.savingCredentialId}
            onSave={props.onSaveCredential}
            activeCredentialId={props.activeCredentialId}
            onCredentialOpen={props.onCredentialOpen}
          />}
        </TabsContent>
        <TabsContent value="rules" className="pt-4"><RulesWorkspace ruleSets={props.ruleSets} rules={props.rules} attachments={props.attachments} providers={props.providers} scopeProviderId={props.provider.id} mutations={props.ruleMutations} /></TabsContent>
        <TabsContent value="routing" className="pt-4"><ProviderRoutingRules provider={props.provider} channel={props.channel} rules={props.routingRules} /></TabsContent>
        <TabsContent value="settings" className="pt-4">
          <Card size="sm"><CardHeader><CardTitle>{t("providers.tabs.settings")}</CardTitle><CardDescription>{t("providers.settings.description")}</CardDescription></CardHeader><CardContent className="flex justify-end">{edit}</CardContent></Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}
