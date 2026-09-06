import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { createOAuthClient, deleteOAuthClient, oauthClients, updateOAuthClient } from "@/api/oauth"
import type { OAuthClientDto } from "@/generated/OAuthClientDto"
import type { OAuthClientWriteRequest } from "@/generated/OAuthClientWriteRequest"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { QueryState } from "@/components/query-state"
import { OAuthClientForm } from "@/components/settings/oauth-client-form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

type Change = { kind: "create"; value: OAuthClientWriteRequest } | { kind: "update"; client: OAuthClientDto; value: OAuthClientWriteRequest } | { kind: "delete"; client: OAuthClientDto }

export function OAuthClientsCard() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState<number | null>(null)
  const [creating, setCreating] = useState(false)
  const [confirmation, setConfirmation] = useState<Change | null>(null)
  const query = useQuery({ queryKey: ["admin", "oauth-clients"], queryFn: ({ signal }) => oauthClients(signal) })
  const mutation = useMutation({
    mutationFn: async (change: Change) => {
      if (change.kind === "create") await createOAuthClient(change.value)
      else if (change.kind === "update") await updateOAuthClient(change.client.id, change.value)
      else await deleteOAuthClient(change.client.id)
    },
    onSuccess: async () => {
      setConfirmation(null); setCreating(false); setEditing(null)
      await Promise.all([queryClient.invalidateQueries({ queryKey: ["admin", "oauth-clients"] }), queryClient.invalidateQueries({ queryKey: ["portal", "oauth-sessions"] })])
      toast.success(t("settings.oauth.saved"))
    },
    onError: () => toast.error(t("settings.oauth.saveError")),
  })
  const status = (client: OAuthClientDto) => <Badge variant={client.enabled ? "secondary" : "outline"}>{t(client.enabled ? "common.status.enabled" : "common.status.disabled")}</Badge>
  const actions = (client: OAuthClientDto) => <div className="flex gap-2"><Button type="button" size="sm" variant="outline" disabled={mutation.isPending} onClick={() => { setCreating(false); setEditing(editing === client.id ? null : client.id) }}>{t("common.actions.edit")}</Button><Button type="button" size="sm" variant="outline" disabled={mutation.isPending} onClick={() => setConfirmation({ kind: "delete", client })}>{t("common.actions.delete")}</Button></div>
  const columns: Array<DataTableColumn<OAuthClientDto>> = [
    { key: "name", label: t("settings.oauth.name"), header: t("settings.oauth.name"), cell: (client) => client.name },
    { key: "client", label: t("settings.oauth.clientId"), header: t("settings.oauth.clientId"), cell: (client) => <code>{client.client_id}</code> },
    { key: "redirects", label: t("settings.oauth.redirects"), header: t("settings.oauth.redirects"), cell: (client) => client.redirect_uris.length },
    { key: "status", label: t("common.status.label"), header: t("common.status.label"), cell: status },
    { key: "actions", label: t("settings.oauth.actions"), header: t("settings.oauth.actions"), cell: actions },
  ]
  const save = (client: OAuthClientDto, value: OAuthClientWriteRequest) => {
    const change: Change = { kind: "update", client, value }
    if (client.enabled && !value.enabled) setConfirmation(change)
    else mutation.mutate(change)
  }
  return (
    <Card>
      <CardHeader><CardTitle>{t("settings.oauth.title")}</CardTitle><CardDescription>{t("settings.oauth.description")}</CardDescription></CardHeader>
      <CardContent className="flex flex-col gap-4">
        <QueryState loading={query.isLoading} error={query.isError ? t("settings.oauth.loadError") : ""}>
          {creating ? <OAuthClientForm key="new" pending={mutation.isPending} onSave={(value) => mutation.mutate({ kind: "create", value })} onCancel={() => setCreating(false)} /> : null}
          <DataTable columns={columns} rows={query.data ?? []} rowKey={(client) => client.id} searchText={(client) => `${client.name} ${client.client_id}`} storageKey="settings-oauth-clients" empty={t("settings.oauth.empty")} selectable={false}
            createAction={<Button type="button" variant="outline" disabled={creating || mutation.isPending} onClick={() => { setEditing(null); setCreating(true) }}>{t("settings.oauth.addClient")}</Button>}
            activeRowKey={editing} onRowClick={(client) => { setCreating(false); setEditing(editing === client.id ? null : client.id) }}
            renderExpandedRow={(client) => <OAuthClientForm key={client.id} client={client} pending={mutation.isPending} onSave={(value) => save(client, value)} onCancel={() => setEditing(null)} />}
            renderCard={(client) => <div className="flex flex-col gap-2"><p>{client.name}</p><code className="break-all">{client.client_id}</code>{status(client)}{actions(client)}</div>} />
        </QueryState>
      </CardContent>
      <ConfirmDangerous open={confirmation != null} onOpenChange={(open) => { if (!open) setConfirmation(null) }} title={t("settings.oauth.revokeTitle")} description={t("settings.oauth.revokeDescription")} confirmLabel={t("settings.oauth.confirm")} pending={mutation.isPending} onConfirm={() => { if (confirmation) mutation.mutate(confirmation) }} />
    </Card>
  )
}
