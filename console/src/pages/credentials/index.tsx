import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials, connectionProfiles } from "@/api/configuration"
import { credentialDirectory, resetHealth } from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { useConsoleContext } from "@/capability/session"
import { CollectionPage } from "@/pages/identity/collection"
import { credentialFields } from "@/pages/providers/fields"
import { InstantCell } from "@/components/cells"
import { Button } from "@/components/ui/button"
import { ErrorNotice, QueryState } from "@/components/state"
import { CredentialDetails } from "./details"
import { CredentialLoginDialog } from "./login"
import { CredentialForm } from "./form"
import { CredentialBulkImport } from "./bulk-import"
import { CredentialTest } from "./test"

export function ProviderCredentials({ providerId }: { providerId: string }) {
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  const provider = providers.data?.find(row => row.id === providerId)
  return <QueryState isPending={providers.isPending} error={providers.error}>{provider ? <CredentialCollection provider={provider} /> : null}</QueryState>
}
function CredentialCollection({ provider }: { provider: CredentialProviderDto }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const refresh = () => client.invalidateQueries({ queryKey: ["admin", "/credentials"] })
  const health = useMutation({ mutationFn: resetHealth, onSuccess: async (_, id) => {
    await Promise.all([refresh(), client.invalidateQueries({ queryKey: ["credential-limits", id] })])
  } })
  const toggle = useMutation({ mutationFn: (row: CredentialDto) => credentials.update(row.id, { enabled: !row.enabled }), onSuccess: refresh })
  const busy = health.isPending || toggle.isPending
  const [detail, setDetail] = useState<{ row: CredentialDto; tab: "basic" | "upstream" } | null>(null)
  const [testing, setTesting] = useState<CredentialDto | null>(null)
  const [login, setLogin] = useState(false)
  const [importing, setImporting] = useState(false)
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap gap-2"><Button variant="outline" disabled={!provider.enabled} onClick={() => setImporting(true)}>{t("credentialImport.title")}</Button>
    {provider.enabled && provider.loginModes.some(mode => mode !== "api_key") ? <Button className="self-start" onClick={() => setLogin(true)}>{t("management.loginAdd")}</Button> : null}</div>
    {health.error || toggle.error ? <ErrorNotice error={health.error || toggle.error} /> : null}
    <CollectionPage creatable={provider.enabled} embedded id="credentials" family={credentials} filter={{ providerId: provider.id }} create={body => {
      if (!provider.enabled) return Promise.reject(new Error(t("management.disabled")))
      return credentials.create({ ...body, providerId: provider.id })
    }} rowId={row => row.id} rowLabel={row => row.label ?? row.id} searchable
      columns={[{ key: "label", cell: row => row.label ?? row.id }, { key: "authKind", cell: row => row.authKind }, { key: "connectionProfileId", cell: row => <ProfileName id={row.connectionProfileId} /> }, { key: "expiresAtMs", cell: row => <InstantCell value={row.expiresAtMs} /> }, { key: "status", cell: row => <Button size="sm" variant={row.status === "dead" ? "destructive" : "secondary"} title={t("management.healthReset")} aria-label={`${t("management.healthReset")}: ${row.label ?? row.id}`} disabled={busy} onClick={() => health.mutate(row.id)}>{t(`values.${row.status}`)}</Button> }, { key: "enabled", cell: row => <Button size="sm" variant={row.enabled ? "secondary" : "outline"} aria-label={`${t("fields.enabled")}: ${row.label ?? row.id}`} aria-pressed={row.enabled} disabled={busy} onClick={() => toggle.mutate(row)}>{t(row.enabled ? "fields.enabled" : "management.disabled")}</Button> }]}
      fields={credentialFields} renderForm={props => <CredentialForm {...props} providerId={provider.id} channel={provider.channel} />} onOpen={row => setDetail({ row, tab: "basic" })} onEdit={row => setDetail({ row, tab: "basic" })}
      rowActions={row => <><Button variant="ghost" size="sm" onClick={() => setDetail({ row, tab: "upstream" })}>{t("limits.upstream")}</Button><Button variant="ghost" size="sm" disabled={!provider.enabled || !row.hasSecret} onClick={() => setTesting(row)}>{t("providers.models.test")}</Button></>}
    />
    {detail ? <CredentialDetails credential={detail.row} initialTab={detail.tab} provider={provider} onClose={() => setDetail(null)} /> : null}
    {testing ? <CredentialTest credential={testing} onClose={() => setTesting(null)} /> : null}
    {importing ? <CredentialBulkImport provider={provider} onClose={() => setImporting(false)} /> : null}
    {login ? <CredentialLoginDialog provider={provider} onClose={() => setLogin(false)} /> : null}
  </div>
}

function ProfileName({ id }: { id: string | null }) {
  const { t } = useTranslation(), context = useConsoleContext()
  const profile = useQuery({ queryKey: ["admin", "/connection-profiles", "detail", id], queryFn: () => connectionProfiles.get(id!), enabled: !!id && context.has("configuration.connection-profiles") })
  return profile.data?.name ?? id ?? t("form.unset")
}
