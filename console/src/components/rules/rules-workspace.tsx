import { LinkIcon, PencilIcon, PlusIcon, Trash2Icon, UnlinkIcon } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { ProviderRuleSetWriteRequest } from "@/generated/ProviderRuleSetWriteRequest"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import { ApplicationPresetButton } from "@/components/rules/application-preset-button"
import { AttachmentDialog } from "@/components/rules/attachment-dialog"
import { RuleList } from "@/components/rules/rule-list"
import { RuleSetDialog } from "@/components/rules/rule-set-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { BatchActions } from "@/components/batch-actions"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { cn } from "@/lib/utils"

export type RuleMutations = {
  saving: boolean
  saveSet: Parameters<typeof RuleSetDialog>[0]["onSave"]
  deleteSet: (id: number) => void
  saveRule: Parameters<typeof RuleList>[0]["onSave"]
  deleteRule: (id: number) => void
  attach: (value: ProviderRuleSetWriteRequest, id?: number) => Promise<void>
  detach: (id: number) => void
}

type Props = {
  ruleSets: Array<RuleSetDto>
  rules: Array<RuleDto>
  attachments: Array<ProviderRuleSetDto>
  providers: Array<ProviderDto>
  scopeProviderId?: number
  mutations: RuleMutations
}

export function RulesWorkspace(props: Props) {
  const { t } = useTranslation()
  const scopedAttachments = props.scopeProviderId == null
    ? props.attachments
    : props.attachments.filter((attachment) => attachment.provider_id === props.scopeProviderId)
  const visibleIds = props.scopeProviderId == null ? null : new Set(scopedAttachments.map((value) => value.rule_set_id))
  const visibleSets = visibleIds == null ? props.ruleSets : props.ruleSets.filter((set) => visibleIds.has(set.id))
  const [selectedId, setSelectedId] = useState<number | null | undefined>(undefined)
  const effectiveSelectedId = selectedId === undefined ? visibleSets[0]?.id : selectedId
  const selected = visibleSets.find((set) => set.id === effectiveSelectedId) ?? (selectedId == null ? null : visibleSets[0] ?? null)
  const selectedAttachments = selected ? props.attachments.filter((value) => value.rule_set_id === selected.id) : []
  const scopedAttachment = props.scopeProviderId == null ? null : selectedAttachments.find((value) => value.provider_id === props.scopeProviderId) ?? null
  const providerNames = useMemo(() => new Map(props.providers.map((provider) => [provider.id, provider.name])), [props.providers])
  const unattached = props.ruleSets.filter((set) => !scopedAttachments.some((value) => value.rule_set_id === set.id))
  const columns: Array<DataTableColumn<RuleSetDto>> = [
    { key: "name", label: t("rules.fields.name"), header: t("rules.fields.name"), cell: (set) => <span className="font-medium">{ruleSetText(set, "name", t)}</span> },
    { key: "scope", label: t("rules.fields.scope"), header: t("rules.fields.scope"), cell: (set) => <Badge variant="secondary">{scopeLabel(props.attachments.filter((value) => value.rule_set_id === set.id).length, t)}</Badge> },
  ]

  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap justify-end gap-2">
      {props.scopeProviderId == null
        ? <RuleSetDialog saving={props.mutations.saving} onSave={props.mutations.saveSet} trigger={<Button><PlusIcon data-icon="inline-start" />{t("rules.sets.add")}</Button>} />
        : <><ApplicationPresetButton providerId={props.scopeProviderId} /><AttachmentDialog providers={props.providers} ruleSets={unattached} fixedProviderId={props.scopeProviderId} saving={props.mutations.saving} onSave={props.mutations.attach} trigger={<Button disabled={!unattached.length}><LinkIcon data-icon="inline-start" />{t("rules.attachments.attachExisting")}</Button>} /></>}
    </div>
    <div className="grid min-w-0 gap-5 md:grid-cols-[minmax(16rem,0.7fr)_minmax(0,1.3fr)]">
      <div className={cn(selected && "hidden md:block")}>
        <DataTable columns={columns} rows={visibleSets} rowKey={(set) => set.id} searchText={(set) => `${ruleSetText(set, "name", t)} ${ruleSetText(set, "description", t)}`} renderCard={(set) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="truncate font-medium">{ruleSetText(set, "name", t)}</p><p className="truncate text-xs text-muted-foreground">{ruleSetText(set, "description", t)}</p></div><Badge variant="secondary">{scopeLabel(props.attachments.filter((value) => value.rule_set_id === set.id).length, t)}</Badge></div>} empty={t(props.scopeProviderId == null ? "rules.sets.empty" : "rules.attachments.empty")} storageKey={props.scopeProviderId == null ? "rule-sets" : `provider-${props.scopeProviderId}-rule-sets`} activeRowKey={selected?.id} selectable batchActions={(rows) => <BatchActions entity="rule-sets" rows={rows} queryKeys={["rule-sets", "rules", "provider-rule-sets"]} remove={props.scopeProviderId == null} />} onRowClick={(set) => setSelectedId(set.id)} />
      </div>
      <div className={cn("min-w-0", !selected && "hidden md:block")}>
        {selected ? <div className="flex flex-col gap-4">
          <Button className="self-start md:hidden" variant="ghost" onClick={() => setSelectedId(null)}>{t("common.actions.back")}</Button>
          <Card><CardHeader><CardTitle>{ruleSetText(selected, "name", t)}</CardTitle><CardDescription>{ruleSetText(selected, "description", t)}</CardDescription></CardHeader><CardContent className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-2"><Badge>{scopeLabel(selectedAttachments.length, t)}</Badge>{scopedAttachment?.inherited ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}{!selected.enabled || scopedAttachment?.enabled === false ? <Badge variant="secondary">{t("common.status.disabled")}</Badge> : null}</div>
            <div className="flex flex-wrap gap-2">
              <RuleSetDialog ruleSet={selected} saving={props.mutations.saving} onSave={props.mutations.saveSet} trigger={<Button size="sm" variant="outline"><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>} />
              {props.scopeProviderId == null ? <AttachmentDialog providers={props.providers} ruleSets={[selected]} fixedRuleSetId={selected.id} saving={props.mutations.saving} onSave={props.mutations.attach} trigger={<Button size="sm" variant="outline"><LinkIcon data-icon="inline-start" />{t("rules.attachments.attach")}</Button>} /> : null}
              {scopedAttachment ? <Button size="sm" variant="outline" onClick={() => detach(scopedAttachment)}><UnlinkIcon data-icon="inline-start" />{t("rules.attachments.detach")}</Button> : null}
              <Button size="sm" variant="ghost" disabled={props.rules.some((rule) => rule.rule_set_id === selected.id) || selectedAttachments.length > 0} onClick={() => props.mutations.deleteSet(selected.id)}><Trash2Icon data-icon="inline-start" />{t("common.actions.delete")}</Button>
            </div>
            {props.scopeProviderId == null && selectedAttachments.length ? <div><p className="mb-2 text-sm font-medium">{t("rules.attachments.title")}</p><div className="flex flex-wrap gap-2">{selectedAttachments.map((value) => <Badge key={value.id} variant="outline">{providerNames.get(value.provider_id) ?? value.provider_id}</Badge>)}</div></div> : null}
          </CardContent></Card>
          <RuleList ruleSetId={selected.id} rules={props.rules.filter((rule) => rule.rule_set_id === selected.id)} inherited={scopedAttachment?.inherited ?? false} saving={props.mutations.saving} onSave={props.mutations.saveRule} onDelete={props.mutations.deleteRule} />
        </div> : <div className="grid min-h-80 place-items-center text-sm text-muted-foreground">{t("rules.sets.selectPrompt")}</div>}
      </div>
    </div>
  </div>

  function detach(attachment: ProviderRuleSetDto) {
    if (!attachment.inherited) return props.mutations.detach(attachment.id)
    void props.mutations.attach({ provider_id: attachment.provider_id, rule_set_id: attachment.rule_set_id, sort_order: attachment.sort_order, enabled: false }, attachment.id)
  }
}

function scopeLabel(count: number, t: (key: string) => string) {
  if (count === 0) return t("rules.scope.unused")
  if (count === 1) return t("rules.scope.private")
  return t("rules.scope.shared")
}

function ruleSetText(ruleSet: RuleSetDto, field: "name" | "description", t: (key: string) => string) {
  const marker = ruleSet.description?.startsWith("gproxy:channel-default:") ? ruleSet.description : null
  if (marker) {
    const key = marker.slice("gproxy:channel-default:".length).replaceAll(":", "_")
    return t(`rules.channelDefaults.${key}.${field}`)
  }
  return field === "name" ? ruleSet.name : ruleSet.description ?? t("rules.sets.noDescription")
}
