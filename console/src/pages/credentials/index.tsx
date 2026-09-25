import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials, connectionProfiles } from "@/api/configuration"
import { credentialDirectory } from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { useConsoleContext } from "@/capability/session"
import { CollectionPage } from "@/pages/identity/collection"
import { credentialFields } from "@/pages/providers/fields"
import { BoolCell, InstantCell } from "@/components/cells"
import { Button } from "@/components/ui/button"
import { QueryState } from "@/components/state"
import { CredentialDetails } from "./details"
import { CredentialLoginDialog } from "./login"
import { CredentialForm } from "./form"
import { CredentialTest } from "./test"

export function ProviderCredentials({ providerId }: { providerId: string }) {
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory })
  const provider = providers.data?.find(row => row.id === providerId)
  return <QueryState isPending={providers.isPending} error={providers.error}>{provider ? <CredentialCollection provider={provider} /> : null}</QueryState>
}
function CredentialCollection({ provider }: { provider: CredentialProviderDto }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const profiles = useQuery({ queryKey: ["admin", "/connection-profiles", "directory"], queryFn: connectionProfiles, enabled: context.has("configuration.connection-profiles") })
  const [detail, setDetail] = useState<{ row: CredentialDto; tab: "basic" | "limits" } | null>(null)
  const [testing, setTesting] = useState<CredentialDto | null>(null)
  const [login, setLogin] = useState(false)
  return <div className="flex flex-col gap-4">
    {provider.enabled && provider.loginModes.some(mode => mode !== "api_key") ? <Button className="self-start" onClick={() => setLogin(true)}>{t("management.loginAdd")}</Button> : null}
    <CollectionPage creatable={provider.enabled} embedded id="credentials" family={credentials} filter={{ providerId: provider.id }} create={body => {
      if (!provider.enabled) return Promise.reject(new Error(t("management.disabled")))
      return credentials.create({ ...body, providerId: provider.id })
    }} rowId={row => row.id} rowLabel={row => row.label ?? row.id} searchable
      columns={[{ key: "label", cell: row => row.label ?? row.id }, { key: "authKind", cell: row => row.authKind }, { key: "connectionProfileId", cell: row => profiles.data?.find(profile => profile.id === row.connectionProfileId)?.name ?? row.connectionProfileId ?? t("form.unset") }, { key: "expiresAtMs", cell: row => <InstantCell value={row.expiresAtMs} /> }, { key: "status", cell: row => t(`values.${row.status}`) }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
      fields={credentialFields} renderForm={props => <CredentialForm {...props} providerId={provider.id} />} onOpen={row => setDetail({ row, tab: "basic" })} onEdit={row => setDetail({ row, tab: "basic" })}
      rowActions={row => <><Button variant="ghost" size="sm" onClick={() => setDetail({ row, tab: "limits" })}>{t("limits.local")}</Button><Button variant="ghost" size="sm" disabled={!provider.enabled || !row.hasSecret} onClick={() => setTesting(row)}>{t("providers.models.test")}</Button></>}
    />
    {detail ? <CredentialDetails credential={detail.row} initialTab={detail.tab} provider={provider} onClose={() => setDetail(null)} /> : null}
    {testing ? <CredentialTest credential={testing} onClose={() => setTesting(null)} /> : null}
    {login ? <CredentialLoginDialog provider={provider} onClose={() => setLogin(false)} /> : null}
  </div>
}
