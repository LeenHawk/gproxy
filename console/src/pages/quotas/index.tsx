import { useState } from "react"
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { budgetStatus, quotas, resetQuota } from "@/api/quotas"
import { directory } from "@/api/models"
import { invalidateConfiguration } from "@/api/invalidation"
import { credentialLimits } from "@/api/credentials"
import type { QuotaDto } from "@/generated/sdk"
import { useConsoleContext } from "@/capability/session"
import { formatInstant } from "@/lib/format"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { ManagementDialog } from "@/components/management-dialog"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { QuotaEditor } from "./editor"

export function QuotasPanel({ ownerKind, ownerId, providerId, runtimeAvailable = true }: { ownerKind: string; ownerId: string; providerId?: string; runtimeAvailable?: boolean }) {
  const { t, i18n } = useTranslation()
  const context = useConsoleContext()
  const client = useQueryClient()
  const limit = ownerKind === "provider" || ownerKind === "credential"
  const credential = ownerKind === "credential"
  const instance = context.scope?.kind === "instance"
  const editable = context.has("configuration.quotas.write") && (!limit || instance)
  const readRules = !credential || instance
  const rules = useQuery({ queryKey: ["admin", "/quotas", ownerKind, ownerId], queryFn: () => directory(quotas, { ownerKind, ownerId }), enabled: readRules })
  const defaults = useQuery({ queryKey: ["admin", "/quotas", "provider", providerId], queryFn: () => directory(quotas, { ownerKind: "provider", ownerId: providerId! }), enabled: credential && instance && !!providerId })
  const status = useQuery({ queryKey: ["quota-status", ownerKind, ownerId], queryFn: () => budgetStatus(ownerKind, ownerId), enabled: !limit })
  const limits = useQuery({ queryKey: ["credential-limits", ownerId], queryFn: () => credentialLimits(ownerId), enabled: credential && runtimeAvailable })
  const own = rules.data ?? []
  const [editing, setEditing] = useState<{ row?: QuotaDto; override: boolean } | null>(null)
  const [confirm, setConfirm] = useState<{ row: QuotaDto; kind: "delete" | "reset" } | null>(null)
  const refresh = () => invalidateConfiguration(client, "/quotas")
  const save = useMutation({ mutationFn: async (body: Parameters<typeof quotas.create>[0]) => {
    const target = editing?.override ? own.find(row => row.windowKey === editing.row?.windowKey) : editing?.row
    const name = body.windowKey ?? target?.windowKey
    if ((!target || (body.windowKey !== undefined && body.windowKey !== target.windowKey)) && own.some(row => row.id !== target?.id && row.windowKey === name)) throw new Error(t("limits.nameTaken"))
    return target ? quotas.update(target.id, body) : quotas.create({ ...body, ownerKind, ownerId })
  }, onSuccess: async () => { await refresh(); setEditing(null); toast.success(t("toast.saved")) } })
  const action = useMutation({ mutationFn: ({ row, kind }: NonNullable<typeof confirm>) => kind === "delete" ? quotas.remove(row.id) : resetQuota(row), onSuccess: async () => { await refresh(); setConfirm(null); toast.success(t("toast.saved")) } })
  const overrides = new Set(own.filter(row => row.enabled).map(row => row.windowKey))
  const inherited = (defaults.data ?? []).filter(row => row.enabled && !overrides.has(row.windowKey))
  const effective = runtimeAvailable ? limits.data ?? [] : []
  const rows: QuotaDto[] = readRules ? [...inherited, ...own] : effective.map(row => ({ id: row.quotaId, ownerKind: row.ownerKind, ownerId: row.ownerId, windowKey: row.windowKey, metric: row.metric, unit: row.unit, limitValue: row.limit, period: row.period, periodSeconds: null, anchorAtMs: null, modelPattern: row.modelPattern, enabled: true }))
  const busy = save.isPending || action.isPending
  const pending = (readRules && rules.isPending) || (credential && instance && !!providerId && defaults.isPending) || (!limit && status.isPending) || (credential && runtimeAvailable && limits.isPending)
  const error = rules.error ?? defaults.error ?? status.error ?? (runtimeAvailable ? limits.error : null)
  const open = (row?: QuotaDto, override = false) => { save.reset(); setConfirm(null); setEditing({ row, override }) }
  return <div className="flex flex-col gap-4">
    {!limit ? <p className="text-sm text-muted-foreground">{t("limits.budgetHelp")}</p> : null}
    {credential && !runtimeAvailable ? <p className="text-sm text-muted-foreground">{t("limits.notRunning")}</p> : null}
    {!editable ? <p className="text-sm text-muted-foreground">{t("limits.readOnly")}</p> : null}
    <QueryState isPending={pending} error={error}>
      {editing ? <QuotaEditor key={editing.row?.id ?? "new"} original={editing.row} limit={limit} override={editing.override} pending={busy} error={save.error} onSave={body => save.mutate(body)} onCancel={() => setEditing(null)} /> : <>
        {editable ? <Button className="self-end" disabled={busy} onClick={() => open()}>{t(limit ? "limits.add" : "limits.addBudget")}</Button> : null}
        {!rows.length ? <EmptyNotice title={t(limit ? "limits.empty" : "limits.emptyBudget")} /> : null}
        {rows.map(row => {
          const fromProvider = credential && row.ownerKind === "provider"
          const hasDefault = credential && row.ownerKind === "credential" && defaults.data?.some(item => item.enabled && item.windowKey === row.windowKey)
          const inheritedDefault = hasDefault && row.enabled && !own.some(item => item.id !== row.id && item.enabled && item.windowKey === row.windowKey)
          const current = row.enabled ? (credential ? effective.find(item => item.quotaId === row.id) : status.data?.find(item => item.quotaId === row.id)) : undefined
          const resetsAt = current ? ("windowEndMs" in current ? current.windowEndMs : current.resetsAtMs) : undefined
          return <Card key={row.id} className="gap-3 py-4"><CardHeader className="px-4"><CardTitle className="flex flex-wrap items-center gap-2"><span className="break-all">{row.windowKey}</span><Badge variant={row.enabled ? "secondary" : "outline"}>{t(row.enabled ? "limits.enabled" : "limits.disabled")}</Badge>{credential ? <Badge variant="outline">{t(fromProvider ? "limits.inherited" : "limits.custom")}</Badge> : null}</CardTitle></CardHeader><CardContent className="flex flex-col gap-3 px-4">
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm"><dt className="text-muted-foreground">{t(row.metric === "requests" ? "limits.requestLimit" : "limits.costLimit")}</dt><dd className="break-all">{row.limitValue} {row.unit}</dd><dt className="text-muted-foreground">{t("fields.period")}</dt><dd>{t(`userQuota.periods.${row.period}`, { defaultValue: row.period })}{row.periodSeconds ? ` (${row.periodSeconds}s)` : ""}</dd><dt className="text-muted-foreground">{t("limits.appliesTo")}</dt><dd className="break-all">{row.modelPattern ?? t("limits.allModels")}</dd>
              {ownerKind !== "provider" ? <><dt className="text-muted-foreground">{t("limits.used")}</dt><dd>{current?.used ?? "—"} {row.unit}</dd><dt className="text-muted-foreground">{t("limits.resetsAt")}</dt><dd>{current ? resetsAt == null ? t("limits.never") : formatInstant(resetsAt, i18n.language) : "—"}</dd></> : null}
            </dl>
            {hasDefault && !row.enabled ? <p className="text-sm text-muted-foreground">{t("limits.disabledOverride")}</p> : null}
            {editable ? <div className="flex flex-wrap gap-2">
              <Button size="sm" variant="outline" disabled={busy} onClick={() => open(row, fromProvider)}>{t(fromProvider ? "limits.override" : "actions.edit")}</Button>
              {!fromProvider ? <><Button size="sm" variant="ghost" disabled={busy || !row.enabled || (credential && !runtimeAvailable)} onClick={() => { action.reset(); setConfirm({ row, kind: "reset" }) }}>{t(ownerKind === "provider" ? "limits.resetAll" : "limits.reset")}</Button><Button size="sm" variant="ghost" disabled={busy} onClick={() => { action.reset(); setConfirm({ row, kind: "delete" }) }}>{t(inheritedDefault ? "limits.restore" : "actions.delete")}</Button></> : null}
            </div> : null}
            {confirm?.row.id === row.id ? <div role="alert" className="flex flex-col gap-3 rounded-lg border p-3"><p className="text-sm">{t(confirm.kind === "delete" ? inheritedDefault ? "limits.restoreConfirm" : "confirm.deleteTitle" : ownerKind === "provider" ? "limits.resetAllConfirm" : "limits.resetConfirm", { name: row.windowKey })}</p><div className="flex gap-2"><Button size="sm" variant="outline" disabled={busy} onClick={() => setConfirm(null)}>{t("actions.cancel")}</Button><Button size="sm" variant="destructive" disabled={busy} onClick={() => action.mutate(confirm)}>{t(confirm.kind === "delete" ? inheritedDefault ? "limits.restore" : "actions.delete" : "limits.reset")}</Button></div>{action.error ? <ErrorNotice error={action.error} /> : null}</div> : null}
          </CardContent></Card>
        })}
      </>}
    </QueryState>
  </div>
}

export function QuotaButton({ ownerKind, ownerId, name }: { ownerKind: string; ownerId: string; name: string }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const busy = useIsMutating() > 0
  return <><Button size="sm" variant="ghost" onClick={() => setOpen(true)}>{t("limits.budget")}</Button>{open ? <ManagementDialog className="sm:max-w-2xl" title={`${name} · ${t("limits.budget")}`} busy={busy} onClose={() => setOpen(false)}><QuotasPanel ownerKind={ownerKind} ownerId={ownerId} /></ManagementDialog> : null}</>
}
