import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { IdResponse } from "@/generated/IdResponse"
import type { ProviderDto } from "@/generated/ProviderDto"
import type { ProviderRuleSetDto } from "@/generated/ProviderRuleSetDto"
import type { ProviderRuleSetWriteRequest } from "@/generated/ProviderRuleSetWriteRequest"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { RuleSetWriteRequest } from "@/generated/RuleSetWriteRequest"
import { ApplicationPresetButton } from "@/components/rules/application-preset-button"
import { AttachmentDialog } from "@/components/rules/attachment-dialog"
import { RuleList } from "@/components/rules/rule-list"
import { RuleSetForm } from "@/components/rules/rule-set-form"
import { BatchActions } from "@/components/batch-actions"
import { EntityDeleteButton } from "@/components/entity-delete-button"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

export type RuleMutations = {
  saving: boolean
  saveSet: (value: RuleSetWriteRequest, id?: number) => Promise<IdResponse | undefined>
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
  const location = useAdminLocation()
  const embedded = props.scopeProviderId != null
  const creating = !embedded && location.segments[0] === "new"
  const scopedAttachments = embedded
    ? props.attachments.filter((attachment) => attachment.provider_id === props.scopeProviderId)
    : props.attachments
  const visibleIds = embedded ? new Set(scopedAttachments.map((attachment) => attachment.rule_set_id)) : null
  const visibleSets = visibleIds == null ? props.ruleSets : props.ruleSets.filter((set) => visibleIds.has(set.id))
  const [localSelectedId, setLocalSelectedId] = useState<number | null>(null)
  const routedId = Number(location.segments[0])
  const selectedId = embedded ? localSelectedId : Number.isFinite(routedId) ? routedId : null
  const selected = visibleSets.find((set) => set.id === selectedId) ?? null
  const selectedAttachments = selected
    ? props.attachments.filter((attachment) => attachment.rule_set_id === selected.id)
    : []
  const providerNames = useMemo(
    () => new Map(props.providers.map((provider) => [provider.id, provider.name])),
    [props.providers],
  )
  const unattached = props.ruleSets.filter(
    (set) => !scopedAttachments.some((attachment) => attachment.rule_set_id === set.id),
  )
  const detailTab = location.segments[1] === "providers" || location.segments[1] === "settings"
    ? location.segments[1]
    : "rules"

  const detach = (attachment: ProviderRuleSetDto) => {
    if (!attachment.inherited) {
      props.mutations.detach(attachment.id)
      return
    }
    void props.mutations.attach({
      provider_id: attachment.provider_id,
      rule_set_id: attachment.rule_set_id,
      sort_order: attachment.sort_order,
      enabled: false,
    }, attachment.id)
  }

  const createAction = embedded ? <div className="flex items-center gap-1">
    <ApplicationPresetButton providerId={props.scopeProviderId!} />
    <AttachmentDialog
      providers={props.providers}
      ruleSets={unattached}
      fixedProviderId={props.scopeProviderId}
      saving={props.mutations.saving}
      onSave={props.mutations.attach}
      trigger={<Button size="icon-sm" disabled={!unattached.length} aria-label={t("rules.attachments.attachExisting")}><PlusIcon aria-hidden /></Button>}
    />
  </div> : <Button size="icon-sm" aria-label={t("rules.sets.add")} onClick={() => navigateAdminPath("/admin/rules/new/settings")}><PlusIcon aria-hidden /></Button>

  const back = () => embedded ? setLocalSelectedId(null) : navigateAdminPath(adminPath("rules"))
  return <WorkspaceLayout
    storageKey={embedded ? `gproxy.workspace.provider-${props.scopeProviderId}-rules.width` : "gproxy.workspace.rules.width"}
    title={t("rules.sets.title")}
    items={visibleSets}
    selectedId={selected?.id ?? null}
    getSearchText={(set) => `${ruleSetText(set, "name", t)} ${ruleSetText(set, "description", t)}`}
    renderTitle={(set) => ruleSetText(set, "name", t)}
    renderSummary={(set) => ruleSetText(set, "description", t)}
    renderAction={(set) => {
      const count = props.attachments.filter((attachment) => attachment.rule_set_id === set.id).length
      return <Badge variant="secondary" aria-label={`${t("rules.attachments.title")}: ${count}`}>{count}</Badge>
    }}
    onSelect={(set) => embedded ? setLocalSelectedId(set.id) : navigateAdminPath(`/admin/rules/${set.id}/rules`)}
    onBack={back}
    searchPlaceholder={t("rules.sets.search")}
    emptyLabel={t(embedded ? "rules.attachments.empty" : "rules.sets.empty")}
    resizeLabel={t("rules.sets.resize")}
    selectAllLabel={t("common.dataTable.selectAll")}
    selectRowLabel={(set) => `${t("common.dataTable.selectRow")}: ${ruleSetText(set, "name", t)}`}
    selectedLabel={(count) => t("common.dataTable.selected", { count })}
    mobileBackLabel={t("common.actions.back")}
    createAction={createAction}
    batchActions={embedded ? undefined : (rows, done) => <BatchActions entity="rule-sets" rows={rows} queryKeys={["rule-sets", "rules", "provider-rule-sets"]} onApplied={done} size="xs" />}
    emptyState={<Empty><EmptyHeader><EmptyTitle>{t("rules.sets.title")}</EmptyTitle><EmptyDescription>{t("rules.sets.selectPrompt")}</EmptyDescription></EmptyHeader></Empty>}
    detailOpen={creating || selected != null}
  >
    {creating ? <Card><CardHeader><CardTitle>{t("rules.sets.add")}</CardTitle></CardHeader><CardContent><RuleSetForm saving={props.mutations.saving} onSave={props.mutations.saveSet} onSaved={(result) => { if (result) navigateAdminPath(`/admin/rules/${result.id}/settings`) }} /></CardContent></Card> : null}
    {selected ? <RuleSetDetail
      selected={selected}
      detailTab={detailTab}
      embedded={embedded}
      scopeProviderId={props.scopeProviderId}
      rules={props.rules}
      providers={props.providers}
      attachments={selectedAttachments}
      providerNames={providerNames}
      mutations={props.mutations}
      onDetach={detach}
      onDeleted={back}
    /> : null}
  </WorkspaceLayout>
}

function RuleSetDetail(props: {
  selected: RuleSetDto
  detailTab: string
  embedded: boolean
  scopeProviderId?: number
  rules: Array<RuleDto>
  providers: Array<ProviderDto>
  attachments: Array<ProviderRuleSetDto>
  providerNames: Map<number, string>
  mutations: RuleMutations
  onDetach: (attachment: ProviderRuleSetDto) => void
  onDeleted: () => void
}) {
  const { t } = useTranslation()
  const selectedRules = props.rules.filter((rule) => rule.rule_set_id === props.selected.id)
  const scopedAttachment = props.attachments.find(
    (attachment) => attachment.provider_id === props.scopeProviderId,
  )

  if (props.embedded) return <div className="flex flex-col gap-4">
    <RuleSetSummary set={props.selected} attachments={props.attachments} scopedAttachment={scopedAttachment} />
    {scopedAttachment ? <Button className="self-start" size="sm" variant="outline" onClick={() => props.onDetach(scopedAttachment)}>{t("rules.attachments.detach")}</Button> : null}
    <RuleList ruleSetId={props.selected.id} rules={selectedRules} inherited={scopedAttachment?.inherited ?? false} saving={props.mutations.saving} onSave={props.mutations.saveRule} onDelete={props.mutations.deleteRule} />
  </div>

  return <div className="flex flex-col gap-4">
    <RuleSetSummary set={props.selected} attachments={props.attachments} deleteDisabled={selectedRules.length > 0 || props.attachments.length > 0} onDeleted={props.onDeleted} />
    <Tabs value={props.detailTab} onValueChange={(tab) => navigateAdminPath(`/admin/rules/${props.selected.id}/${tab}`, true)}>
      <TabsList variant="line">
        <TabsTrigger value="rules">{t("rules.entries.title")}</TabsTrigger>
        <TabsTrigger value="providers">{t("rules.attachments.title")}</TabsTrigger>
        <TabsTrigger value="settings">{t("rules.sets.settings")}</TabsTrigger>
      </TabsList>
      <TabsContent value="rules" className="pt-4">
        <RuleList ruleSetId={props.selected.id} rules={selectedRules} inherited={false} saving={props.mutations.saving} onSave={props.mutations.saveRule} onDelete={props.mutations.deleteRule} />
      </TabsContent>
      <TabsContent value="providers" className="pt-4">
        <Card>
          <CardHeader>
            <CardTitle>{t("rules.attachments.title")}</CardTitle>
            <CardAction><AttachmentDialog providers={props.providers} ruleSets={[props.selected]} fixedRuleSetId={props.selected.id} saving={props.mutations.saving} onSave={props.mutations.attach} trigger={<Button size="sm">{t("rules.attachments.attach")}</Button>} /></CardAction>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            {props.attachments.length ? props.attachments.map((attachment) => <div key={attachment.id} className="flex items-center justify-between gap-3 rounded-md border p-3">
              <div className="min-w-0"><p className="truncate text-sm font-medium">{props.providerNames.get(attachment.provider_id) ?? `#${attachment.provider_id}`}</p><p className="text-xs text-muted-foreground">{attachment.inherited ? t("rules.values.inherited") : t("rules.attachments.title")}</p></div>
              <Button size="sm" variant="outline" onClick={() => props.onDetach(attachment)}>{t("rules.attachments.detach")}</Button>
            </div>) : <p className="text-sm text-muted-foreground">{t("rules.attachments.emptyForSet")}</p>}
          </CardContent>
        </Card>
      </TabsContent>
      <TabsContent value="settings" className="pt-4">
        <Card>
          <CardHeader><CardTitle>{t("rules.sets.settings")}</CardTitle></CardHeader>
          <CardContent><RuleSetForm key={`${props.selected.id}-${props.selected.name}-${props.selected.description}-${props.selected.enabled}`} ruleSet={props.selected} saving={props.mutations.saving} onSave={props.mutations.saveSet} /></CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  </div>
}

function RuleSetSummary({ set, attachments, scopedAttachment, deleteDisabled, onDeleted }: {
  set: RuleSetDto
  attachments: Array<ProviderRuleSetDto>
  scopedAttachment?: ProviderRuleSetDto
  deleteDisabled?: boolean
  onDeleted?: () => void
}) {
  const { t } = useTranslation()
  return <header className="flex flex-wrap items-start justify-between gap-3">
    <div><h2 className="text-xl font-semibold">{ruleSetText(set, "name", t)}</h2><p className="mt-1 text-sm text-muted-foreground">{ruleSetText(set, "description", t)}</p></div>
    <div className="flex flex-wrap items-center gap-2"><Badge aria-label={`${t("rules.attachments.title")}: ${attachments.length}`}>{attachments.length}</Badge>{scopedAttachment?.inherited ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}{!set.enabled || scopedAttachment?.enabled === false ? <Badge variant="secondary">{t("common.status.disabled")}</Badge> : null}{onDeleted ? <EntityDeleteButton entity="rule-sets" id={set.id} label={ruleSetText(set, "name", t)} queryKeys={["rule-sets", "rules", "provider-rule-sets"]} disabled={deleteDisabled} onDeleted={onDeleted} /> : null}</div>
  </header>
}

function ruleSetText(set: RuleSetDto, field: "name" | "description", t: (key: string) => string) {
  return field === "name" ? set.name : set.description ?? t("rules.sets.noDescription")
}
