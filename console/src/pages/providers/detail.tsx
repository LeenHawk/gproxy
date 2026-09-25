import { invalidateConfiguration } from "@/api/invalidation"
import { QuotasPanel } from "@/pages/quotas"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Boxes, KeyRound, Settings2 } from "lucide-react"
import { toast } from "sonner"
import { channels, providers, providerPath } from "@/api/configuration"
import type { ChannelDescriptor, ProviderDto } from "@/generated/sdk"
import { ConfirmButton } from "@/components/confirm"
import { Page, PageHeader, PageSection } from "@/components/page"
import { ProviderForm } from "@/pages/providers/provider-form"
import { ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ProviderCredentials } from "@/pages/credentials"
import { ProviderRules, ProviderOperations, ProviderEndpoints } from "@/pages/providers/rules"
import { ProviderModels } from "@/pages/providers/models"
import { useNavigate } from "@/lib/router"

export function ProviderDetailPage({ providerId, tab }: { providerId: string; tab: string }) {
  const provider = useQuery({ queryKey: ["admin", "/providers", "detail", providerId], queryFn: () => providers.get(providerId) })
  const catalog = useQuery({ queryKey: ["configuration", "channels"], queryFn: channels })
  return (
    <QueryState isPending={provider.isPending || catalog.isPending} error={provider.error ?? catalog.error}>
      {provider.data ? <ProviderDetail provider={provider.data} catalog={catalog.data ?? []} tab={tab} /> : null}
    </QueryState>
  )
}

function ProviderDetail({ provider, catalog, tab }: { provider: ProviderDto; catalog: Array<ChannelDescriptor>; tab: string }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const navigate = useNavigate()
  const channel = catalog.find((entry) => entry.id === provider.channel)
  const saved = () => {
    void client.invalidateQueries({ queryKey: ["admin", "/providers"] })
    void client.invalidateQueries({ queryKey: ["credential-providers"] })
    toast.success(t("toast.saved"))
  }
  const update = useMutation({
    mutationFn: (body: Parameters<typeof providers.update>[1]) => providers.update(provider.id, body),
    onSuccess: saved,
  })
  const remove = useMutation({
    mutationFn: () => providers.remove(provider.id),
    onSuccess: () => {
      client.removeQueries({ queryKey: ["admin", "/providers", "detail", provider.id], exact: true })
      void invalidateConfiguration(client, "/providers")
      toast.success(t("toast.deleted"))
      navigate("/providers")
    },
  })
  return (
    <Page>
      <PageHeader
        title={provider.displayName ?? provider.name}
        actions={<>
          <Badge variant="outline">{channel?.displayName ?? provider.channel}</Badge>
          <Switch aria-label={t("fields.enabled")} checked={provider.enabled} disabled={update.isPending} onCheckedChange={(enabled) => update.mutate({ enabled })} />
          <ConfirmButton title={t("confirm.deleteTitle", { name: provider.displayName ?? provider.name })} disabled={remove.isPending} onConfirm={() => remove.mutate()}>{t("actions.delete")}</ConfirmButton>
        </>}
      />
      <p className="break-all text-sm text-muted-foreground">{t("providers.routePrefix")}: <code>/{provider.name}</code></p>
      {update.error ? <ErrorNotice error={update.error} /> : null}
      {remove.error ? <ErrorNotice error={remove.error} /> : null}
      <Tabs value={tab} onValueChange={(value) => navigate(`${providerPath(provider.id)}/${value}`)} className="gap-6">
        <TabsList variant="line" className="max-w-full flex-wrap justify-start group-data-horizontal/tabs:h-auto">
          <TabsTrigger value="credentials"><KeyRound />{t("nav.credentials")}</TabsTrigger>
          <TabsTrigger value="models"><Boxes />{t("nav.provider-models")}</TabsTrigger>
          <TabsTrigger value="rules">{t("nav.provider-rule-sets")}</TabsTrigger>
          <TabsTrigger value="routing">{t("nav.operation-rules")}</TabsTrigger>
          <TabsTrigger value="settings"><Settings2 />{t("providers.settings")}</TabsTrigger>
        </TabsList>
        <TabsContent value="rules"><ProviderRules providerId={provider.id} providerName={provider.name} /></TabsContent>
        <TabsContent value="routing"><ProviderOperations providerId={provider.id} /></TabsContent>
        <TabsContent value="credentials"><ProviderCredentials providerId={provider.id} /></TabsContent>
        <TabsContent value="models"><ProviderModels provider={provider} /></TabsContent>
        <TabsContent value="settings" className="flex flex-col gap-6">
          <ProviderForm key={`${provider.id}-${JSON.stringify(provider)}`} provider={provider} catalog={catalog} onSubmit={(body) => update.mutate(body)} pending={update.isPending} error={update.error} />
          <PageSection title={t("limits.providerDefaults")}><QuotasPanel ownerKind="provider" ownerId={provider.id} /></PageSection>
          <PageSection title={t("nav.operation-endpoints")}><ProviderEndpoints providerId={provider.id} /></PageSection>
        </TabsContent>
      </Tabs>
    </Page>
  )
}
