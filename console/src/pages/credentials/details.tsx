import { useState } from "react"
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { credentials } from "@/api/configuration"
import * as actions from "@/api/credentials"
import type { CredentialDto, DiscoveredModelDto, ModelTestResultDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { useConsoleContext } from "@/capability/session"
import { EgressTest } from "@/components/proxy-control"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { QuotasPanel } from "@/pages/quotas"
import { CredentialForm } from "./form"
import { UpstreamQuota } from "./upstream"

export function CredentialDetails({ credential, provider, initialTab = "basic", onClose }: { credential: CredentialDto; provider: CredentialProviderDto; initialTab?: "basic" | "limits" | "upstream"; onClose: () => void }) {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const client = useQueryClient()
  const id = credential.id
  const row = useQuery({ queryKey: ["admin", "/credentials", id], queryFn: () => credentials.get(id) })
  const [tab, setTab] = useState(initialTab)
  const [maintenance, setMaintenance] = useState<"secret" | "status" | "health" | "delete" | "test" | null>(null)
  const [reason, setReason] = useState("")
  const [status, setStatus] = useState(credential.status)
  const [model, setModel] = useState("")
  const [models, setModels] = useState<DiscoveredModelDto[]>([])
  const [testResult, setTestResult] = useState<ModelTestResultDto | null>(null)
  const [secret, setSecret] = useState<string | null>(null)
  const [revealing, setRevealing] = useState(false)
  const [revealError, setRevealError] = useState<unknown>(null)
  const refresh = () => client.invalidateQueries({ queryKey: ["admin", "/credentials"] })
  const save = useMutation({ mutationFn: (body: Record<string, unknown>) => credentials.update(id, body), onSuccess: async () => { await refresh(); toast.success(t("toast.saved")) } })
  const action = useMutation({ mutationFn: async (kind: "refresh" | "forceRefresh" | "health" | "status" | "delete" | "discover" | "test") => {
    if (kind === "refresh" || kind === "forceRefresh") await actions.refreshCredential(id, kind === "forceRefresh")
    if (kind === "health") await actions.resetHealth(id)
    if (kind === "status") await actions.credentialStatus(id, status, reason.trim() || null)
    if (kind === "delete") await credentials.remove(id)
    if (kind === "discover") setModels(await actions.discoverWithCredential(id))
    if (kind === "test") setTestResult(await actions.testWithCredential(id, model))
  }, onSuccess: async (_, kind) => {
    if (kind === "delete") { client.removeQueries({ queryKey: ["admin", "/credentials", id], exact: true }); onClose(); await refresh(); return }
    await refresh()
    await client.invalidateQueries({ queryKey: ["credential-limits", id] })
    if (kind !== "discover" && kind !== "test") { setMaintenance(null); toast.success(t("toast.saved")) }
  } })
  const mutations = useIsMutating()
  const busy = mutations > 0 || revealing
  async function reveal() {
    setMaintenance("secret"); setSecret(null); setRevealError(null); setRevealing(true)
    try { setSecret(JSON.stringify(await actions.revealCredential(id), null, 2)) } catch (error) { setRevealError(error) } finally { setRevealing(false) }
  }
  const current = row.data ?? credential
  return <ManagementDialog className="sm:max-w-3xl" title={`${current.label ?? id} · ${provider.name}`} titleAside={<Badge variant="outline">{t(`values.${current.status}`)}</Badge>} onClose={onClose} busy={busy}>
    <div className="flex flex-wrap items-center justify-end gap-2"><DropdownMenu><DropdownMenuTrigger asChild><Button variant="outline" size="sm" disabled={busy}>{t("limits.more")}</Button></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuGroup>
      <DropdownMenuItem disabled={!current.hasSecret} onSelect={() => void reveal()}>{t("management.reveal")}</DropdownMenuItem>
      {provider.capabilities.refresh ? <>{(["refresh", "forceRefresh"] as const).map(kind => <DropdownMenuItem key={kind} disabled={!provider.enabled} onSelect={() => action.mutate(kind)}>{t(`management.${kind}`)}</DropdownMenuItem>)}</> : null}
      <DropdownMenuItem onSelect={() => { setStatus(current.status); setMaintenance("status"); action.reset() }}>{t("limits.changeStatus")}</DropdownMenuItem>
      <DropdownMenuItem onSelect={() => { setMaintenance("health"); action.reset() }}>{t("management.healthReset")}</DropdownMenuItem>
      <DropdownMenuItem disabled={!provider.enabled} onSelect={() => { setMaintenance("test"); action.reset() }}>{t("management.test")}</DropdownMenuItem>
      <DropdownMenuItem variant="destructive" onSelect={() => { setMaintenance("delete"); action.reset() }}>{t("actions.delete")}</DropdownMenuItem>
    </DropdownMenuGroup></DropdownMenuContent></DropdownMenu></div>
    {action.error ? <ErrorNotice error={action.error} /> : null}
    {maintenance ? <div className="flex flex-col gap-3 rounded-lg border p-3">
      <div className="flex items-center justify-between gap-2"><h3 className="font-medium">{t(`limits.maintenance.${maintenance}`)}</h3><Button size="sm" variant="ghost" disabled={busy} onClick={() => { setMaintenance(null); setSecret(null) }}>{t("actions.close")}</Button></div>
      {maintenance === "secret" ? <>{revealError ? <ErrorNotice error={revealError} /> : null}<Textarea aria-label={t("management.reveal")} readOnly value={secret ?? ""} rows={6} className="font-mono" /></> : null}
      {maintenance === "status" ? <FieldGroup><Field><FieldLabel htmlFor="credential-status">{t("fields.status")}</FieldLabel><Select value={status} onValueChange={setStatus} disabled={busy}><SelectTrigger id="credential-status"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{["active", "dead"].map(value => <SelectItem key={value} value={value}>{t(`values.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="status-reason">{t("management.statusReason")}</FieldLabel><Input id="status-reason" value={reason} disabled={busy} onChange={event => setReason(event.target.value)} /></Field><Button disabled={busy} onClick={() => action.mutate("status")}>{t("actions.save")}</Button></FieldGroup> : null}
      {maintenance === "health" || maintenance === "delete" ? <><p className="text-sm">{t(maintenance === "health" ? "management.healthResetConfirm" : "confirm.deleteTitle", { name: current.label ?? id })}</p><Button variant="destructive" className="self-start" disabled={busy} onClick={() => action.mutate(maintenance)}>{t(maintenance === "health" ? "management.healthReset" : "actions.delete")}</Button></> : null}
      {maintenance === "test" ? <><Field><FieldLabel htmlFor="credential-test-model">{t("fields.upstreamName")}</FieldLabel><Input id="credential-test-model" list="credential-test-models" disabled={busy} value={model} onChange={event => setModel(event.target.value)} /><datalist id="credential-test-models">{models.map(item => <option key={item.upstreamName} value={item.upstreamName} />)}</datalist></Field><div className="flex flex-wrap gap-2"><Button variant="outline" disabled={busy} onClick={() => action.mutate("discover")}>{t("management.discover")}</Button><Button disabled={busy || !model.trim()} onClick={() => action.mutate("test")}>{t("management.test")}</Button>{context.has("configuration.connectivity") ? <EgressTest scope={{ scope: "credential", credential_id: id }} /> : null}</div>{testResult ? <div role="status" className="text-sm"><p>{testResult.model} · {testResult.ok ? t("modelUI.testOk") : testResult.error} · {testResult.status} · {testResult.latencyMs} ms</p>{testResult.reply ? <p className="whitespace-pre-wrap">{testResult.reply}</p> : null}</div> : null}</> : null}
    </div> : null}
    <QueryState isPending={row.isPending} error={row.error}>
      <Tabs value={tab} onValueChange={value => setTab(value as typeof tab)}><TabsList variant="line" className="max-w-full"><TabsTrigger value="basic" disabled={busy}>{t("limits.basic")}</TabsTrigger><TabsTrigger value="limits" disabled={busy}>{t("limits.local")}</TabsTrigger><TabsTrigger value="upstream" disabled={busy}>{t("limits.upstream")}</TabsTrigger></TabsList>
        <TabsContent value="basic" forceMount hidden={tab !== "basic"}><CredentialForm key={JSON.stringify(current)} inline open original={current} providerId={provider.id} onOpenChange={() => {}} onSubmit={body => save.mutate(body)} pending={save.isPending} error={save.error} /></TabsContent>
        <TabsContent value="limits">{tab === "limits" ? <QuotasPanel ownerKind="credential" ownerId={id} providerId={provider.id} runtimeAvailable={current.enabled && provider.enabled && current.status === "active"} /> : null}</TabsContent>
        <TabsContent value="upstream">{tab === "upstream" ? <UpstreamQuota id={id} provider={provider} /> : null}</TabsContent>
      </Tabs>
    </QueryState>
  </ManagementDialog>
}
