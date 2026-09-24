import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { credentials } from "@/api/configuration"
import * as actions from "@/api/credentials"
import type { CredentialDto } from "@/generated/sdk"
import type { CredentialProviderDto } from "@/generated/app"
import { EgressTest } from "@/components/proxy-control"
import { ManagementDialog } from "@/components/management-dialog"
import { ConfirmButton } from "@/components/confirm"
import { SecretDialog } from "@/components/secret-dialog"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import { DataTable } from "@/components/data-table"
import { QuotasPanel } from "@/pages/quotas"
import { useConsoleContext } from "@/capability/session"

export function CredentialDetails({ credential, provider, onClose }: { credential: CredentialDto; provider: CredentialProviderDto; onClose: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const context = useConsoleContext()
  const id = credential.id
  const row = useQuery({ queryKey: ["admin", "/credentials", id], queryFn: () => credentials.get(id) })
  const quota = useQuery({ queryKey: ["credential-quota", id], queryFn: () => actions.credentialQuota(id) })
  const limits = useQuery({ queryKey: ["credential-limits", id], queryFn: () => actions.credentialLimits(id) })
  const [reason, setReason] = useState("")
  const [model, setModel] = useState("")
  const [secret, setSecret] = useState<string | null>(null)
  const [revealing, setRevealing] = useState(false)
  const [revealError, setRevealError] = useState<unknown>(null)
  const [result, setResult] = useState<unknown>(null)
  const updated = useMutation({ mutationFn: async (kind: string) => {
    if (kind === "refresh") return actions.refreshCredential(id, false)
    if (kind === "forceRefresh") return actions.refreshCredential(id, true)
    if (kind === "probe") return actions.probeQuota(id)
    if (kind === "upstreamReset") return actions.resetUpstreamQuota(id)
    if (kind === "healthReset") return actions.resetHealth(id)
    if (kind === "discover") return actions.discoverWithCredential(id)
    if (kind === "test") return actions.testWithCredential(id, model)
    return actions.credentialStatus(id, kind, reason.trim() || null)
  }, onSuccess: async value => { setResult(value); await Promise.all([client.invalidateQueries({ queryKey: ["admin", "/credentials"] }), quota.refetch(), limits.refetch()]) } })
  const busy = updated.isPending || revealing
  const reveal = async () => { setRevealing(true); setRevealError(null); try { setSecret(JSON.stringify(await actions.revealCredential(id), null, 2)) } catch (error) { setRevealError(error) } finally { setRevealing(false) } }
  return <ManagementDialog title={credential.label ?? id} onClose={onClose} busy={busy}>
    <QueryState isPending={row.isPending} error={row.error}>
      <p>{t("fields.status")}: {row.data?.status} · {row.data?.statusReason}</p>
      <div className="flex flex-wrap gap-2">
        {context.has("configuration.connectivity") ? <EgressTest scope={{ scope: "credential", credential_id: id }} /> : null}
        <Button variant="outline" disabled={busy || !row.data?.hasSecret} onClick={() => void reveal()}>{t("management.reveal")}</Button>
        {provider.capabilities.refresh ? <>{["refresh", "forceRefresh"].map(kind => <Button key={kind} variant="outline" disabled={busy} onClick={() => updated.mutate(kind)}>{t(`management.${kind}`)}</Button>)}</> : null}
        {provider.capabilities.quotaQuery ? <Button variant="outline" disabled={busy} onClick={() => updated.mutate("probe")}>{t("management.probe")}</Button> : null}
        {provider.capabilities.quotaReset ? <ConfirmButton disabled={busy} title={t("management.upstreamResetConfirm")} onConfirm={() => updated.mutate("upstreamReset")}>{t("management.upstreamReset")}</ConfirmButton> : null}
        <ConfirmButton disabled={busy} title={t("management.healthResetConfirm")} onConfirm={() => updated.mutate("healthReset")}>{t("management.healthReset")}</ConfirmButton>
      </div>
      <Field><FieldLabel htmlFor="status-reason">{t("management.statusReason")}</FieldLabel><Input id="status-reason" value={reason} onChange={e => setReason(e.target.value)} /></Field>
      <div className="flex gap-2">{["active", "dead"].map(kind => <ConfirmButton key={kind} disabled={busy || row.data?.status === kind} title={t("management.changeStatus", { status: t(`values.${kind}`) })} confirmLabel={t("actions.save")} onConfirm={() => updated.mutate(kind)}>{t(`values.${kind}`)}</ConfirmButton>)}</div>
    </QueryState>
    <h3 className="font-medium">{t("management.recordedQuota")}</h3>
    <QueryState isPending={quota.isPending} error={quota.error}><DataTable empty={<EmptyNotice title={t("state.emptyTitle")} />} rows={quota.data?.cycles ?? []} rowKey={cycle => cycle.id} columns={[{ key: "observedAtMs", cell: cycle => new Date(cycle.observedAtMs).toLocaleString() }, { key: "resetsAtMs", cell: cycle => cycle.resetsAtMs ? new Date(cycle.resetsAtMs).toLocaleString() : "—" }, { key: "snapshot", header: t("management.snapshot"), cell: cycle => <details><summary>{t("management.manage")}</summary><pre className="max-w-lg whitespace-pre-wrap break-all text-xs">{JSON.stringify(cycle.snapshot, null, 2)}</pre></details> }]} />
      <DataTable empty={<EmptyNotice title={t("state.emptyTitle")} />} rows={quota.data?.blocks ?? []} rowKey={block => block.id} columns={[{ key: "operation", cell: block => block.operation ?? "*" }, { key: "untilMs", header: t("fields.expiresAtMs"), cell: block => new Date(block.untilMs).toLocaleString() }]} />
    </QueryState>
    <h3 className="font-medium">{t("management.localLimits")}</h3>
    <QueryState isPending={limits.isPending} error={limits.error}><DataTable empty={<EmptyNotice title={t("state.emptyTitle")} />} rows={limits.data ?? []} rowKey={limit => limit.quotaId} columns={[{ key: "windowKey", cell: limit => limit.windowKey }, { key: "metric", cell: limit => limit.metric }, { key: "limitValue", cell: limit => `${limit.used} / ${limit.limit} ${limit.unit}` }, { key: "modelPattern", cell: limit => limit.modelPattern ?? "*" }]} /></QueryState>
    {context.scope?.kind === "instance" ? <QuotasPanel ownerKind="credential" ownerId={id} /> : null}
    <Field><FieldLabel htmlFor="credential-test-model">{t("fields.upstreamName")}</FieldLabel><Input id="credential-test-model" value={model} onChange={e => setModel(e.target.value)} /></Field>
    <div className="flex gap-2"><Button variant="outline" disabled={busy} onClick={() => updated.mutate("discover")}>{t("management.discover")}</Button><Button disabled={busy || !model.trim()} onClick={() => updated.mutate("test")}>{t("management.test")}</Button></div>
    {result != null ? <details open><summary>{t("management.result")}</summary><pre className="whitespace-pre-wrap break-all text-xs">{JSON.stringify(result, null, 2)}</pre></details> : null}
    {updated.error || revealError ? <ErrorNotice error={updated.error ?? revealError} /> : null}
    <SecretDialog token={secret} onClose={() => setSecret(null)} />
  </ManagementDialog>
}
