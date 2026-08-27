import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { CredentialCycleList } from "@/components/providers/credential-cycle-list"
import { CredentialDialog } from "@/components/providers/credential-dialog"
import { CredentialRowActions } from "@/components/providers/credential-row-actions"
import { QueryState } from "@/components/query-state"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"

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
    { key: "name", label: t("common.name"), header: t("common.name"), cell: (credential) => <div><p className="font-mono text-xs">{credential.label ?? t("providers.credentials.unnamed", { id: credential.id })}</p><p className="font-mono text-xs text-muted-foreground">#{credential.id}</p></div> },
    { key: "health", label: t("common.status.label"), header: t("common.status.label"), cell: (credential) => <StatusBadge status={credential.health} /> },
    { key: "actions", label: t("common.actions.edit"), header: <span className="sr-only">{t("common.actions.edit")}</span>, cell: (credential) => <CredentialRowActions credential={credential} channel={props.channel} presets={props.presets} saving={props.savingCredentialId === credential.id} onSave={props.onSave} />, className: "text-right" },
    { key: "kind", label: t("providers.credentials.kind"), header: t("providers.credentials.kind"), cell: (credential) => <span className="text-xs">{t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })}</span> },
    { key: "weight", label: t("providers.credentials.weight"), header: t("providers.credentials.weight"), cell: (credential) => <span className="font-mono text-xs">{credential.weight}</span> },
    { key: "quota", label: t("usage.credentialCycles"), header: t("usage.credentialCycles"), cell: (credential) => <CredentialCycleList cycles={props.cyclesByCredential.get(credential.id) ?? []} loading={props.cyclesLoading} error={props.cyclesError} /> },
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
          trigger={<Button variant="outline" size="sm"><PlusIcon data-icon="inline-start" />{t("providers.credentials.add")}</Button>}
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
          renderCard={(credential) => <div className="flex flex-col gap-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><p className="truncate font-mono text-xs">{credential.label ?? t("providers.credentials.unnamed", { id: credential.id })}</p><p className="font-mono text-xs text-muted-foreground">{t(`providers.credentials.kinds.${credential.kind}`, { defaultValue: credential.kind })} · #{credential.id}</p></div><StatusBadge status={credential.health} /></div><CredentialCycleList cycles={props.cyclesByCredential.get(credential.id) ?? []} loading={props.cyclesLoading} error={props.cyclesError} /><CredentialRowActions credential={credential} channel={props.channel} presets={props.presets} saving={props.savingCredentialId === credential.id} onSave={props.onSave} /></div>}
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
