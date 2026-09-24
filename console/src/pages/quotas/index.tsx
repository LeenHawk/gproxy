import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { configFamily } from "@/api/config-family"
import { api, json, query } from "@/api/client"
import { apiKeys } from "@/api/admin"
import { credentials } from "@/api/configuration"
import { credentialDirectory, credentialLimits } from "@/api/credentials"
import { directory } from "@/api/models"
import type { BudgetStatusDto, QuotaDto, QuotaPatch, QuotaWrite } from "@/generated/sdk"
import { useConsoleContext } from "@/capability/session"
import { useOwnerChoices } from "@/pages/credentials/ownership"
import { CollectionPage } from "@/pages/identity/collection"
import { Page, PageHeader } from "@/components/page"
import { RecordDialog, type FormField } from "@/components/record-form"
import { ConfirmButton } from "@/components/confirm"
import { BoolCell } from "@/components/cells"
import { ErrorNotice } from "@/components/state"
import { ManagementDialog } from "@/components/management-dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

const quotas = configFamily<QuotaDto, Partial<QuotaWrite>, Partial<QuotaPatch>>("/quotas")
export function QuotasPage() {
  const { t } = useTranslation()
  const context = useConsoleContext()
  const owners = useOwnerChoices()
  const instance = context.scope?.kind === "instance"
  const providers = useQuery({ queryKey: ["credential-providers"], queryFn: credentialDirectory, enabled: instance })
  const keys = useQuery({ queryKey: ["admin", "/api-keys", "directory"], queryFn: () => directory(apiKeys), enabled: instance })
  const accounts = useQuery({ queryKey: ["admin", "/credentials", "directory"], queryFn: () => directory(credentials), enabled: instance })
  const choices = [...owners.choices.filter(entry => entry.value !== "shared"),
    ...(keys.data ?? []).map(row => ({ value: `api_key:${row.id}`, label: `API Key: ${row.name ?? row.id}` })),
    ...(providers.data ?? []).map(row => ({ value: `provider:${row.id}`, label: `${t("fields.providerId")}: ${row.name}` })),
    ...(accounts.data ?? []).map(row => ({ value: `credential:${row.id}`, label: `${t("nav.credentials")}: ${row.label ?? row.id}` })),
    ...(instance ? [{ value: "pool:", label: t("management.pool") }] : []),
  ]
  const [owner, setOwner] = useState(instance ? "" : owners.defaultOwner)
  const [pool, setPool] = useState("")
  const [kind, ...parts] = owner.split(":")
  const id = kind === "pool" ? pool.trim() : parts.join(":")
  return <Page><PageHeader title={t("nav.quotas")} /><Select value={owner} onValueChange={setOwner}><SelectTrigger aria-label={t("management.owner")}><SelectValue placeholder={t("management.owner")} /></SelectTrigger><SelectContent><SelectGroup>{choices.map(entry => <SelectItem key={entry.value} value={entry.value}>{entry.label}</SelectItem>)}</SelectGroup></SelectContent></Select>
    {kind === "pool" ? <Input aria-label={t("fields.ownerId")} placeholder={t("fields.ownerId")} value={pool} onChange={e => setPool(e.target.value)} /> : null}
    {owners.error || providers.error || keys.error || accounts.error ? <ErrorNotice error={owners.error ?? providers.error ?? keys.error ?? accounts.error} /> : null}
    {kind && id ? <QuotasPanel key={`${kind}:${id}`} ownerKind={kind} ownerId={id} /> : null}
  </Page>
}
export function QuotasPanel({ ownerKind, ownerId }: { ownerKind: string; ownerId: string }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const limit = ["provider", "credential"].includes(ownerKind)
  const status = useQuery({ queryKey: ["quota-status", ownerKind, ownerId], queryFn: () => api<BudgetStatusDto[]>(`/admin/api/quotas/status${query({ owners: `${ownerKind}:${ownerId}` })}`), enabled: !limit })
  const limits = useQuery({ queryKey: ["credential-limits", ownerId], queryFn: () => credentialLimits(ownerId), enabled: ownerKind === "credential" })
  const reset = useMutation({ mutationFn: (row: QuotaDto) => api(`/admin/api/quotas/${encodeURIComponent(row.id)}/${limit ? "limit-reset" : "reset"}`, json("POST", {})), onSuccess: () => Promise.all([client.invalidateQueries({ queryKey: ["quota-status"] }), client.invalidateQueries({ queryKey: ["credential-limits"] }), client.invalidateQueries({ queryKey: ["portal", "quota"] })]) })
  const fields: FormField[] = [
    { name: "windowKey", kind: "text", required: true },
    ...(limit ? [{ name: "metric", kind: "select" as const, required: true, choices: ["cost", "requests"].map(value => ({ value, label: t(`management.${value}`) })) }] : []),
    { name: "limitValue", kind: "text", required: true },
    { name: "period", kind: "select", required: true, choices: ["5h", "1d", "7d", "1m", "total", "custom"].map(value => ({ value, label: t(`userQuota.periods.${value}`) })) },
    { name: "periodSeconds", kind: "number", nullable: true }, { name: "anchorAtMs", kind: "datetime", nullable: true },
    { name: "modelPattern", kind: "text", nullable: true }, { name: "enabled", kind: "switch" },
  ]
  return <div className="flex flex-col gap-3"><CollectionPage embedded id="quotas" family={quotas} filter={{ ownerKind, ownerId }} rowId={row => row.id} rowLabel={row => row.windowKey} fields={fields}
    create={body => quotas.create({ ...body, ownerKind, ownerId })}
    columns={[{ key: "windowKey", cell: row => row.windowKey }, { key: "metric", cell: row => row.metric }, { key: "limitValue", cell: row => `${(status.data ?? limits.data)?.find(item => item.quotaId === row.id)?.used ?? "—"} / ${row.limitValue} ${row.unit}` }, { key: "period", cell: row => row.period }, { key: "resetsAtMs", cell: row => { const window = status.data?.find(item => item.quotaId === row.id); return window?.resetsAtMs ? new Date(window.resetsAtMs).toLocaleString() : "—" } }, { key: "modelPattern", cell: row => row.modelPattern ?? "*" }, { key: "enabled", cell: row => <BoolCell value={row.enabled} /> }]}
    renderForm={props => <RecordDialog {...props} mode={props.original ? "edit" : "create"} title={t(props.original ? "edit.quotas" : "create.quotas")} fields={fields} original={props.original ? { ...props.original } : { windowKey: "primary", period: "1m", metric: "cost", enabled: true }} onSubmit={body => {
      const metric = String(body.metric ?? props.original?.metric ?? "cost")
      props.onSubmit({ ...body, ...(!props.original || body.metric !== undefined ? { metric, unit: metric === "requests" ? "count" : "USD" } : {}) })
    }} />}
    rowActions={row => <ConfirmButton disabled={reset.isPending} title={t(limit ? "management.limitResetConfirm" : "userQuota.resetConfirm", { name: row.windowKey })} confirmLabel={t("userQuota.reset")} onConfirm={() => reset.mutate(row)}>{t("userQuota.reset")}</ConfirmButton>}
  />{reset.error || status.error || limits.error ? <ErrorNotice error={reset.error ?? status.error ?? limits.error} /> : null}</div>
}

export function QuotaButton({ ownerKind, ownerId, name }: { ownerKind: string; ownerId: string; name: string }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  return <><Button size="sm" variant="ghost" onClick={() => setOpen(true)}>{t("nav.quotas")}</Button>{open ? <ManagementDialog title={`${name} · ${t("nav.quotas")}`} onClose={() => setOpen(false)}><QuotasPanel ownerKind={ownerKind} ownerId={ownerId} /></ManagementDialog> : null}</>
}
