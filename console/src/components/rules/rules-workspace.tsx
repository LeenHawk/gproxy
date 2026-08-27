import { LinkIcon, PencilIcon, PlusIcon, Trash2Icon, UnlinkIcon } from "lucide-react"
import { useMemo, useState } from "react"
import { useTranslation } from "react-i18next"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import { AttachmentDialog } from "./attachment-dialog"
import { ApplicationPresetButton } from "./application-preset-button"
import { RuleList } from "./rule-list"
import { RuleSetDialog } from "./rule-set-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { DataTable, type DataTableColumn } from "@/components/data-table"
import { BatchActions } from "@/components/batch-actions"
import { cn } from "@/lib/utils"

export type RuleMutations = {
  saving: boolean
  saveSet: Parameters<typeof RuleSetDialog>[0]["onSave"]
  deleteSet: (id: number) => void
  saveRule: Parameters<typeof RuleList>[0]["onSave"]
  deleteRule: (id: number) => void
  attach: Parameters<typeof AttachmentDialog>[0]["onSave"]
  detach: (id: number) => void
}

export function RulesWorkspace({ ruleSets, rules, attachments, providers, scopeProviderId, mutations }: {
  ruleSets: Array<RuleSetDto>
  rules: Array<RuleDto>
  attachments: Array<ProviderRuleSetDto>
  providers: Array<ProviderDto>
  scopeProviderId?: number
  mutations: RuleMutations
}) {
  const { t } = useTranslation()
  const scopedAttachments = scopeProviderId == null ? attachments : attachments.filter((attachment) => attachment.provider_id === scopeProviderId)
  const visibleSetIds = scopeProviderId == null ? null : new Set(scopedAttachments.map((attachment) => attachment.rule_set_id))
  const visibleSets = visibleSetIds == null ? ruleSets : ruleSets.filter((ruleSet) => visibleSetIds.has(ruleSet.id))
  const [selectedId, setSelectedId] = useState<number | null | undefined>(undefined)
  const effectiveSelectedId = selectedId === undefined ? visibleSets[0]?.id : selectedId
  const selected = visibleSets.find((ruleSet) => ruleSet.id === effectiveSelectedId) ?? (selectedId == null ? null : visibleSets[0] ?? null)
  const selectedAttachments = selected ? attachments.filter((attachment) => attachment.rule_set_id === selected.id) : []
  const providerNames = useMemo(() => new Map(providers.map((provider) => [provider.id, provider.name])), [providers])
  const columns: Array<DataTableColumn<RuleSetDto>> = [
    { key: "name", label: t("rules.fields.name"), header: t("rules.fields.name"), cell: (ruleSet) => <span className="font-medium">{ruleSet.name}</span> },
    { key: "scope", label: t("rules.fields.scope"), header: t("rules.fields.scope"), cell: (ruleSet) => <Badge variant="secondary">{scopeLabel(attachments.filter((attachment) => attachment.rule_set_id === ruleSet.id).length, t)}</Badge> },
  ]
  const unattached = ruleSets.filter((ruleSet) => !scopedAttachments.some((attachment) => attachment.rule_set_id === ruleSet.id))
  return <div className="flex flex-col gap-4">
    <div className="flex flex-wrap justify-end gap-2">
      {scopeProviderId == null ? <RuleSetDialog saving={mutations.saving} onSave={mutations.saveSet} trigger={<Button><PlusIcon data-icon="inline-start" />{t("rules.sets.add")}</Button>} /> : <><ApplicationPresetButton providerId={scopeProviderId} /><AttachmentDialog providers={providers} ruleSets={unattached} fixedProviderId={scopeProviderId} saving={mutations.saving} onSave={mutations.attach} trigger={<Button disabled={!unattached.length}><LinkIcon data-icon="inline-start" />{t("rules.attachments.attachExisting")}</Button>} /></>}
    </div>
    <div className="grid min-w-0 gap-5 md:grid-cols-[minmax(16rem,0.7fr)_minmax(0,1.3fr)]">
      <div className={cn(selected && "hidden md:block")}><DataTable columns={columns} rows={visibleSets} rowKey={(ruleSet) => ruleSet.id} searchText={(ruleSet) => `${ruleSet.name} ${ruleSet.description ?? ""}`} renderCard={(ruleSet) => <div className="flex items-center justify-between gap-3"><div className="min-w-0"><p className="truncate font-medium">{ruleSet.name}</p><p className="truncate text-xs text-muted-foreground">{ruleSet.description ?? t("rules.sets.noDescription")}</p></div><Badge variant="secondary">{scopeLabel(attachments.filter((attachment) => attachment.rule_set_id === ruleSet.id).length, t)}</Badge></div>} empty={t(scopeProviderId == null ? "rules.sets.empty" : "rules.attachments.empty")} storageKey={scopeProviderId == null ? "rule-sets" : `provider-${scopeProviderId}-rule-sets`} activeRowKey={selected?.id} selectable batchActions={(rows) => <BatchActions entity="rule-sets" rows={rows} queryKeys={["rule-sets", "rules", "provider-rule-sets"]} remove={scopeProviderId == null} />} onRowClick={(ruleSet) => setSelectedId(ruleSet.id)} /></div>
      <div className={cn("min-w-0", !selected && "hidden md:block")}>
        {selected ? <div className="flex flex-col gap-4"><Button className="self-start md:hidden" variant="ghost" onClick={() => setSelectedId(null)}>{t("common.actions.back")}</Button><Card><CardHeader><CardTitle>{selected.name}</CardTitle><CardDescription>{selected.description ?? t("rules.sets.noDescription")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-3"><div className="flex flex-wrap items-center gap-2"><Badge>{scopeLabel(selectedAttachments.length, t)}</Badge>{scopeProviderId != null ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}{!selected.enabled ? <Badge variant="secondary">{t("common.status.disabled")}</Badge> : null}</div><div className="flex flex-wrap gap-2"><RuleSetDialog ruleSet={selected} saving={mutations.saving} onSave={mutations.saveSet} trigger={<Button size="sm" variant="outline"><PencilIcon data-icon="inline-start" />{t("common.actions.edit")}</Button>} />{scopeProviderId == null ? <AttachmentDialog providers={providers} ruleSets={[selected]} fixedRuleSetId={selected.id} saving={mutations.saving} onSave={mutations.attach} trigger={<Button size="sm" variant="outline"><LinkIcon data-icon="inline-start" />{t("rules.attachments.attach")}</Button>} /> : null}{scopeProviderId != null ? selectedAttachments.filter((attachment) => attachment.provider_id === scopeProviderId).map((attachment) => <Button key={attachment.id} size="sm" variant="outline" onClick={() => mutations.detach(attachment.id)}><UnlinkIcon data-icon="inline-start" />{t("rules.attachments.detach")}</Button>) : null}<Button size="sm" variant="ghost" disabled={rules.some((rule) => rule.rule_set_id === selected.id) || selectedAttachments.length > 0} onClick={() => mutations.deleteSet(selected.id)}><Trash2Icon data-icon="inline-start" />{t("common.actions.delete")}</Button></div>{scopeProviderId == null && selectedAttachments.length ? <div><p className="mb-2 text-sm font-medium">{t("rules.attachments.title")}</p><div className="flex flex-wrap gap-2">{selectedAttachments.map((attachment) => <Badge key={attachment.id} variant="outline">{providerNames.get(attachment.provider_id) ?? attachment.provider_id}</Badge>)}</div></div> : null}</CardContent></Card><RuleList ruleSetId={selected.id} rules={rules.filter((rule) => rule.rule_set_id === selected.id)} inherited={scopeProviderId != null} saving={mutations.saving} onSave={mutations.saveRule} onDelete={mutations.deleteRule} /></div> : <div className="grid min-h-80 place-items-center text-sm text-muted-foreground">{t("rules.sets.selectPrompt")}</div>}
      </div>
    </div>
  </div>
}

function scopeLabel(count: number, t: (key: string) => string) {
  if (count === 0) return t("rules.scope.unused")
  if (count === 1) return t("rules.scope.private")
  return t("rules.scope.shared")
}
