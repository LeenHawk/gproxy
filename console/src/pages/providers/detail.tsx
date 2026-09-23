import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Boxes, KeyRound, Pencil, Settings2 } from "lucide-react"
import { toast } from "sonner"
import { channels, credentials, providers, providerPath, connectionProfiles } from "@/api/configuration"
import type { ChannelDescriptor, ProviderDto } from "@/generated/sdk"
import { BoolCell, InstantCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { Page, PageHeader, PageSection } from "@/components/page"
import { ProviderForm } from "@/pages/providers/provider-form"
import { ErrorNotice, QueryState } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { CollectionPage } from "@/pages/identity/collection"
import { authKinds, credentialFields } from "@/pages/providers/fields"
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
  const profiles = useQuery({ queryKey: ["admin", "/connection-profiles", "directory"], queryFn: connectionProfiles })
  const channel = catalog.find((entry) => entry.id === provider.channel)
  const saved = () => {
    void client.invalidateQueries({ queryKey: ["admin", "/providers"] })
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
      void client.invalidateQueries({ queryKey: ["admin", "/providers"] })
      toast.success(t("toast.deleted"))
      navigate("/providers")
    },
  })
  return (
    <Page>
      <PageHeader
        title={provider.name}
        actions={<>
          <Badge variant="outline">{channel?.displayName ?? provider.channel}</Badge>
          <Switch aria-label={t("fields.enabled")} checked={provider.enabled} disabled={update.isPending} onCheckedChange={(enabled) => update.mutate({ enabled })} />
          <Button size="sm" variant="outline" onClick={() => navigate(`${providerPath(provider.id)}/settings`)}><Pencil data-icon="inline-start" />{t("actions.edit")}</Button>
          <ConfirmButton title={t("confirm.deleteTitle", { name: provider.name })} disabled={remove.isPending} onConfirm={() => remove.mutate()}>{t("actions.delete")}</ConfirmButton>
        </>}
      />
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
        <TabsContent value="rules"><ProviderRules providerId={provider.id} /></TabsContent>
        <TabsContent value="routing"><ProviderOperations providerId={provider.id} /></TabsContent>
        <TabsContent value="credentials"><QueryState isPending={profiles.isPending} error={profiles.error}>
          <CollectionPage
            embedded
            id="credentials"
            family={credentials}
            filter={{ providerId: provider.id }}
            create={(body) => credentials.create({ ...body, providerId: provider.id })}
            searchable
            rowId={(row) => row.id}
            rowLabel={(row) => row.label ?? row.id}
            columns={[
              { key: "label", cell: (row) => row.label ?? row.id },
              { key: "authKind", cell: (row) => authKinds.find((kind) => kind.value === row.authKind)?.label ?? row.authKind },
              { key: "connectionProfileId", cell: (row) => profiles.data?.find((profile) => profile.id === row.connectionProfileId)?.name ?? t("form.unset") },
              { key: "status", cell: (row) => <Badge variant={row.status === "active" ? "success" : "destructive"}>{t(`values.${row.status}`)}</Badge> },
              { key: "enabled", cell: (row) => <BoolCell value={row.enabled} /> },
              { key: "expiresAtMs", cell: (row) => <InstantCell value={row.expiresAtMs} /> },
            ]}
            fields={credentialFields.map((field) => field.name === "connectionProfileId" ? { ...field, kind: "select" as const, choices: (profiles.data ?? []).map((profile) => ({ value: profile.id, label: profile.name })) } : field)}
          />
        </QueryState></TabsContent>
        <TabsContent value="models"><ProviderModels provider={provider} /></TabsContent>
        <TabsContent value="settings" className="flex flex-col gap-6">
          <ProviderForm key={`${provider.id}-${JSON.stringify(provider)}`} provider={provider} catalog={catalog} onSubmit={(body) => update.mutate(body)} pending={update.isPending} error={update.error} />
          <PageSection title={t("nav.operation-endpoints")}><ProviderEndpoints providerId={provider.id} /></PageSection>
        </TabsContent>
      </Tabs>
    </Page>
  )
}
