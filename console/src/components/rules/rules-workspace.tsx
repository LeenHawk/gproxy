import { useMemo, useState } from "react"
import { PlusIcon } from "lucide-react"
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
import { BatchActions } from "@/components/batch-actions"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { WorkspaceLayout } from "@/components/workspace/workspace-layout"
import { adminPath, navigateAdminPath, useAdminLocation } from "@/lib/admin-route"

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
  const location = useAdminLocation()
  const embedded = props.scopeProviderId != null
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
  </div> : <RuleSetDialog
    saving={props.mutations.saving}
    onSave={props.mutations.saveSet}
    trigger={<Button size="icon-sm" aria-label={t("rules.sets.add")}><PlusIcon aria-hidden /></Button>}
  />

  return <WorkspaceLayout
    storageKey={embedded ? `gproxy.workspace.provider-${props.scopeProviderId}-rules.width` : "gproxy.workspace.rules.width"}
    title={t("rules.sets.title")}
    items={visibleSets}
    selectedId={selected?.id ?? null}
    getSearchText={(set) => `${ruleSetText(set, "name", t)} ${ruleSetText(set, "description", t)}`}
    renderTitle={(set) => ruleSetText(set, "name", t)}
    renderSummary={(set) => ruleSetText(set, "description", t)}
    renderAction={(set) => <Badge variant="secondary" aria-label={t("rules.fields.scope")}>{scopeLabel(props.attachments.filter((attachment) => attachment.rule_set_id === set.id).length, t)}</Badge>}
    onSelect={(set) => embedded ? setLocalSelectedId(set.id) : navigateAdminPath(`/admin/rules/${set.id}/rules`)}
    onBack={() => embedded ? setLocalSelectedId(null) : navigateAdminPath(adminPath("rules"))}
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
  >
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
    <RuleSetSummary set={props.selected} attachments={props.attachments} />
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
          <CardContent className="flex flex-wrap gap-2">
            <RuleSetDialog ruleSet={props.selected} saving={props.mutations.saving} onSave={props.mutations.saveSet} trigger={<Button variant="outline">{t("common.actions.edit")}</Button>} />
            <Button variant="ghost" disabled={selectedRules.length > 0 || props.attachments.length > 0} onClick={() => props.mutations.deleteSet(props.selected.id)}>{t("common.actions.delete")}</Button>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  </div>
}

function RuleSetSummary({ set, attachments, scopedAttachment }: {
  set: RuleSetDto
  attachments: Array<ProviderRuleSetDto>
  scopedAttachment?: ProviderRuleSetDto
}) {
  const { t } = useTranslation()
  return <Card>
    <CardHeader><CardTitle>{ruleSetText(set, "name", t)}</CardTitle><CardDescription>{ruleSetText(set, "description", t)}</CardDescription></CardHeader>
    <CardContent className="flex flex-wrap gap-2">
      <Badge>{scopeLabel(attachments.length, t)}</Badge>
      {scopedAttachment?.inherited ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}
      {!set.enabled || scopedAttachment?.enabled === false ? <Badge variant="secondary">{t("common.status.disabled")}</Badge> : null}
    </CardContent>
  </Card>
}

function scopeLabel(count: number, t: (key: string) => string) {
  if (count === 0) return t("rules.scope.unused")
  if (count === 1) return t("rules.scope.private")
  return t("rules.scope.shared")
}

function ruleSetText(set: RuleSetDto, field: "name" | "description", t: (key: string) => string) {
  const marker = set.description?.startsWith("gproxy:channel-default:") ? set.description : null
  if (marker) {
    const key = marker.slice("gproxy:channel-default:".length).replaceAll(":", "_")
    return t(`rules.channelDefaults.${key}.${field}`)
  }
  return field === "name" ? set.name : set.description ?? t("rules.sets.noDescription")
}
