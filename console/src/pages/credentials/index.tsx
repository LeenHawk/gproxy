import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials, connectionProfiles } from "@/api/configuration"
import { credentialDirectory } from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { useConsoleContext } from "@/capability/session"
import { CollectionPage, type CollectionProps } from "@/pages/identity/collection"
import { credentialFields } from "@/pages/providers/fields"
import { BoolCell, InstantCell } from "@/components/cells"
import { RecordDialog } from "@/components/record-form"
import { Button } from "@/components/ui/button"
import { Page, PageHeader } from "@/components/page"
import { QueryState, ErrorNotice } from "@/components/state"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { CredentialDetails } from "./details"
import { CredentialLoginDialog } from "./login"
import { ownerColumns, ownerValue, useOwnerChoices } from "./ownership"

export function CredentialsPage() {
  const { t } = useTranslation()
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  const [selected, setSelected] = useState("")
  const provider = providers.data?.find(row => row.id === selected)
  return <Page><PageHeader title={t("nav.credentials")} /><QueryState isPending={providers.isPending} error={providers.error}>
    <Select value={selected} onValueChange={setSelected}><SelectTrigger aria-label={t("fields.providerId")}><SelectValue placeholder={t("fields.providerId")} /></SelectTrigger><SelectContent><SelectGroup>{providers.data?.map(row => <SelectItem key={row.id} value={row.id}>{row.name}{row.enabled ? "" : ` (${t("management.disabled")})`}</SelectItem>)}</SelectGroup></SelectContent></Select>
    {provider ? <CredentialCollection key={provider.id} provider={provider} /> : null}
  </QueryState></Page>
}
export function ProviderCredentials({ providerId }: { providerId: string }) {
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  const provider = providers.data?.find(row => row.id === providerId)
  return <QueryState isPending={providers.isPending} error={providers.error}>{provider ? <CredentialCollection provider={provider} /> : null}</QueryState>
}
function CredentialCollection({ provider }: { provider: CredentialProviderDto }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const profiles = useQuery({ queryKey: ["admin", "/connection-profiles", "directory"], queryFn: connectionProfiles, enabled: context.has("configuration.connection-profiles") })
  const [detail, setDetail] = useState<CredentialDto | null>(null)
  const [login, setLogin] = useState(false)
  return <div className="flex flex-col gap-4">
    {provider.enabled && provider.loginModes.some(mode => mode !== "api_key") ? <Button className="self-start" onClick={() => setLogin(true)}>{t("management.loginAdd")}</Button> : null}
    <CollectionPage creatable={provider.enabled} embedded id="credentials" family={credentials} filter={{ providerId: provider.id }} create={body => {
      if (!provider.enabled) return Promise.reject(new Error(t("management.disabled")))
      return credentials.create({ ...body, providerId: provider.id })
    }} rowId={row => row.id} rowLabel={row => row.label ?? row.id} searchable
      columns={[{ key: "label", cell: row => row.label ?? row.id }, { key: "authKind", cell: row => row.authKind }, { key: "connectionProfileId", cell: row => profiles.data?.find(profile => profile.id === row.connectionProfileId)?.name ?? row.connectionProfileId ?? t("form.unset") }, { key: "expiresAtMs", cell: row => <InstantCell value={row.expiresAtMs} /> }, { key: "status", cell: row => t(`values.${row.status}`) }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
      fields={credentialFields} renderForm={props => <CredentialForm {...props} providerId={provider.id} />} onOpen={setDetail}
      rowActions={row => <Button variant="ghost" size="sm" onClick={() => setDetail(row)}>{t("management.manage")}</Button>}
    />
    {detail ? <CredentialDetails credential={detail} provider={provider} onClose={() => setDetail(null)} /> : null}
    {login ? <CredentialLoginDialog provider={provider} onClose={() => setLogin(false)} /> : null}
  </div>
}
type FormProps = Parameters<NonNullable<CollectionProps<CredentialDto, unknown, unknown>["renderForm"]>>[0] & { providerId: string }
function CredentialForm({ providerId, ...props }: FormProps) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const ownership = useOwnerChoices()
  const profiles = useQuery({ queryKey: ["admin", "/connection-profiles", "directory"], queryFn: connectionProfiles, enabled: context.has("configuration.connection-profiles") })
  const original = props.original
  const selectedOwner = original ? ownerValue(original) : ownership.defaultOwner
  const choices = ownership.choices.some(row => row.value === selectedOwner) ? ownership.choices : [...ownership.choices, { value: selectedOwner, label: selectedOwner }]
  const fields = credentialFields.filter(field => context.scope?.kind === "instance" || !["connectionProfileId", "proxy"].includes(field.name)).map(field => field.name === "connectionProfileId" ? { ...field, kind: "select" as const, choices: (profiles.data ?? []).map(row => ({ value: row.id, label: row.name })) } : field.name === "proxy" ? { ...field, proxyScope: () => original ? { scope: "credential" as const, credential_id: original.id } : { scope: "provider" as const, provider_id: providerId, parent: true } } : field)
  return <>{ownership.error || profiles.error ? <ErrorNotice error={ownership.error ?? profiles.error} /> : null}<RecordDialog {...props} mode={original ? "edit" : "create"} title={t(original ? "edit.credentials" : "create.credentials")} original={{ ...original, owner: selectedOwner }} fields={[...fields, { name: "owner", label: t("management.owner"), kind: "select", required: true, choices }]}
    onSubmit={body => { const { owner, ...rest } = body; props.onSubmit({ ...rest, ...(owner !== undefined || !original ? ownerColumns(String(owner ?? selectedOwner)) : {}) }) }}
  /></>
}
