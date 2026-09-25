import { useState } from "react"
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { credentials } from "@/api/configuration"
import * as actions from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { ConfirmButton } from "@/components/confirm"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { QuotasPanel } from "@/pages/quotas"
import { CredentialForm } from "./form"
import { UpstreamQuota } from "./upstream"

export function CredentialDetails({ credential, provider, initialTab = "basic", onClose }: { credential: CredentialDto; provider: CredentialProviderDto; initialTab?: "basic" | "limits" | "upstream"; onClose: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const id = credential.id
  const row = useQuery({ queryKey: ["admin", "/credentials", id], queryFn: () => credentials.get(id) })
  const [tab, setTab] = useState(initialTab)
  const [editingStatus, setEditingStatus] = useState(false)
  const [reason, setReason] = useState("")
  const [status, setStatus] = useState(credential.status)
  const [secret, setSecret] = useState<string | null>(null)
  const [revealing, setRevealing] = useState(false)
  const [revealError, setRevealError] = useState<unknown>(null)
  const refresh = () => client.invalidateQueries({ queryKey: ["admin", "/credentials"] })
  const save = useMutation({ mutationFn: (body: Record<string, unknown>) => credentials.update(id, body), onSuccess: async () => { await refresh(); toast.success(t("toast.saved")) } })
  const action = useMutation({ mutationFn: async (kind: "refresh" | "forceRefresh" | "health" | "status") => {
    if (kind === "refresh" || kind === "forceRefresh") await actions.refreshCredential(id, kind === "forceRefresh")
    if (kind === "health") await actions.resetHealth(id)
    if (kind === "status") await actions.credentialStatus(id, status, reason.trim() || null)
  }, onSuccess: async () => {
    await refresh()
    await client.invalidateQueries({ queryKey: ["credential-limits", id] })
    setEditingStatus(false)
    toast.success(t("toast.saved"))
  } })
  const mutations = useIsMutating()
  const busy = mutations > 0 || revealing
  async function reveal() {
    setSecret(null); setRevealError(null); setRevealing(true)
    try { setSecret(JSON.stringify(await actions.revealCredential(id), null, 2)) } catch (error) { setRevealError(error) } finally { setRevealing(false) }
  }
  const current = row.data ?? credential
  return <ManagementDialog className="sm:max-w-3xl" title={`${current.label ?? id} · ${provider.displayName ?? provider.name}`} titleAside={<Badge variant="outline">{t(`values.${current.status}`)}</Badge>} onClose={onClose} busy={busy}>
    {action.error ? <ErrorNotice error={action.error} /> : null}
    <QueryState isPending={row.isPending} error={row.error}>
      <Tabs value={tab} onValueChange={value => setTab(value as typeof tab)}><TabsList variant="line" className="max-w-full"><TabsTrigger value="basic" disabled={busy}>{t("limits.basic")}</TabsTrigger><TabsTrigger value="limits" disabled={busy}>{t("limits.local")}</TabsTrigger><TabsTrigger value="upstream" disabled={busy}>{t("limits.upstream")}</TabsTrigger></TabsList>
        <TabsContent value="basic" forceMount hidden={tab !== "basic"}><CredentialForm secretActions={<div className="flex flex-col gap-3">
          <div className="flex flex-wrap gap-2"><Button variant="outline" size="sm" disabled={busy || !current.hasSecret} onClick={() => void reveal()}>{t("management.reveal")}</Button>
            {provider.capabilities.refresh ? (["refresh", "forceRefresh"] as const).map(kind => <Button key={kind} variant="outline" size="sm" disabled={busy || !provider.enabled} onClick={() => { setSecret(null); action.mutate(kind) }}>{t(`management.${kind}`)}</Button>) : null}
          </div>
          {revealError ? <ErrorNotice error={revealError} /> : null}
          {secret !== null ? <><Textarea aria-label={t("management.reveal")} readOnly value={secret} rows={6} className="font-mono" /><Button className="self-start" size="sm" variant="ghost" disabled={busy} onClick={() => setSecret(null)}>{t("actions.close")}</Button></> : null}
        </div>} key={JSON.stringify(current)} inline open original={current} providerId={provider.id} onOpenChange={() => {}} onSubmit={body => save.mutate(body)} pending={busy} error={save.error} />
          <section className="mt-4 flex flex-col gap-3 border-t pt-4" aria-label={t("fields.status")}>
            <div className="flex flex-wrap items-center gap-2"><span>{t("fields.status")}</span><Badge variant="outline">{t(`values.${current.status}`)}</Badge>
              <Button variant="outline" size="sm" disabled={busy} onClick={() => { setStatus(current.status); setReason(""); setEditingStatus(true); action.reset() }}>{t("limits.changeStatus")}</Button>
              <ConfirmButton variant="outline" disabled={busy} title={t("management.healthResetConfirm")} confirmLabel={t("management.healthReset")} onConfirm={() => action.mutate("health")}>{t("management.healthReset")}</ConfirmButton>
            </div>
            {editingStatus ? <div className="flex flex-col gap-3"><FieldGroup><Field><FieldLabel htmlFor="credential-status">{t("fields.status")}</FieldLabel><Select value={status} onValueChange={setStatus} disabled={busy}><SelectTrigger id="credential-status"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["active", "dead"].map(value => <SelectItem key={value} value={value}>{t(`values.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="status-reason">{t("management.statusReason")}</FieldLabel><Input id="status-reason" value={reason} disabled={busy} onChange={event => setReason(event.target.value)} /></Field><Button disabled={busy} onClick={() => action.mutate("status")}>{t("actions.save")}</Button></FieldGroup><Button className="self-start" variant="ghost" size="sm" disabled={busy} onClick={() => setEditingStatus(false)}>{t("actions.cancel")}</Button></div> : null}
          </section>
        </TabsContent>
        <TabsContent value="limits">{tab === "limits" ? <QuotasPanel ownerKind="credential" ownerId={id} providerId={provider.id} runtimeAvailable={current.enabled && provider.enabled && current.status === "active"} /> : null}</TabsContent>
        <TabsContent value="upstream">{tab === "upstream" ? <UpstreamQuota id={id} provider={provider} /> : null}</TabsContent>
      </Tabs>
    </QueryState>
  </ManagementDialog>
}
