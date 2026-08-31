import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useTranslation } from "react-i18next"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { CredentialModelHealth } from "@/components/providers/credential-model-health"
import { CredentialRowActions } from "@/components/providers/credential-row-actions"
import { QueryState } from "@/components/query-state"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"

type Props = {
  providerId: number
  channel?: ChannelDto
  presets: Array<TlsPresetDto>
  credentials: Array<CredentialDto>
  cyclesByCredential: Map<number, Array<CredentialQuotaCycleDto>>
  credentialsLoading: boolean
  credentialsError: boolean
  cyclesLoading: boolean
  cyclesError: boolean
  savingCredentialId: number | null
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
  activeCredentialId?: number | null
  onCredentialOpen?: (credential: CredentialDto) => void
}

export function CredentialList(props: Props) {
  const { t } = useTranslation()
  const columns: Array<DataTableColumn<CredentialDto>> = [
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (credential) => credential.label ? <div><p className="font-mono text-xs">{credential.label}</p><p className="font-mono text-xs text-muted-foreground">#{credential.id}</p></div> : <p className="font-mono text-xs">{t("providers.credentials.unnamed", { id: credential.id })}</p> },
    { key: "health", label: t("common.status.label"), header: t("common.status.label"), cell: (credential) => <span className="flex items-center gap-2"><StatusBadge status={credential.health} /><CredentialModelHealth values={credential.model_health} /></span> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: (credential) => <CredentialRowActions credential={credential} channel={props.channel} presets={props.presets} saving={props.savingCredentialId === credential.id} onSave={props.onSave} />, className: "text-right" },
    { key: "kind", label: t("providers.credentials.kind"), header: t("providers.credentials.kind"), cell: (credential) => <span className="text-xs">{t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })}</span> },
    { key: "weight", label: t("providers.credentials.weight"), header: t("providers.credentials.weight"), cell: (credential) => <span className="font-mono text-xs">{credential.weight}</span> },
    // The row opens the credential, where the cycles live behind their own control; the list carries the pressure only.
    { key: "quota", label: t("usage.credentialCycles"), header: t("usage.credentialCycles"), cell: (credential) => <QuotaPressure cycles={props.cyclesByCredential.get(credential.id) ?? []} /> },
  ]

  return (
    <section className="flex flex-col gap-3" aria-labelledby={`provider-${props.providerId}-credentials`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 id={`provider-${props.providerId}-credentials`} className="font-medium">{t("providers.credentials.title")}</h3>
        <CredentialDialog
          providerId={props.providerId}
          channel={props.channel}
          presets={props.presets}
          onSave={props.onSave}
          trigger={<Button variant="outline" size="sm">{t("providers.credentials.add")}</Button>}
        />
      </div>
      <QueryState
        loading={props.credentialsLoading}
        error={props.credentialsError ? t("providers.credentials.loadError") : ""}
        empty={!props.credentials.length ? t("providers.credentials.empty") : undefined}
      >
        <DataTable
          columns={columns}
          rows={props.credentials}
          rowKey={(credential) => credential.id}
          searchText={(credential) => `${credential.label ?? ""} ${credential.kind} ${credential.id} ${credential.health}`}
          renderCard={(credential) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="truncate font-mono text-xs">{credential.label ?? t("providers.credentials.unnamed", { id: credential.id })}</p><p className="font-mono text-xs text-muted-foreground">{t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })} · #{credential.id}</p></div><div className="flex items-center gap-2"><QuotaPressure cycles={props.cyclesByCredential.get(credential.id) ?? []} /><StatusBadge status={credential.health} /></div></div><CredentialRowActions credential={credential} channel={props.channel} presets={props.presets} saving={props.savingCredentialId === credential.id} onSave={props.onSave} /></div>}
          empty={t("providers.credentials.empty")}
          storageKey="credentials"
          selectable
          batchActions={(rows) => <BatchActions entity="credentials" rows={rows} queryKeys={["credentials", "credential-cycles"]} />}
          activeRowKey={props.activeCredentialId}
          onRowClick={props.onCredentialOpen}
        />
      </QueryState>
    </section>
  )
}

function QuotaPressure({ cycles }: { cycles: Array<CredentialQuotaCycleDto> }) {
  const { t } = useTranslation()
  if (!cycles.length) return <span className="text-xs text-muted-foreground">{t("common.none")}</span>
  const worst = cycles.reduce((highest, cycle) => Math.max(highest, Number(cycle.used_percent) || 0), 0)
  return <Badge variant={worst >= 80 ? "destructive" : "outline"} className="machine-text">{Math.round(worst)}%</Badge>
}
