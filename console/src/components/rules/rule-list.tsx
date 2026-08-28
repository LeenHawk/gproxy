import { PencilIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleWriteRequest } from "@/generated/RuleWriteRequest"
import { RuleDialog } from "./rule-dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"

const RANK: Record<RuleDto["config"]["kind"], number> = { system_text: 0, cache_breakpoint: 1, rewrite: 2, transform: 3, header: 4 }

export function RuleList({ ruleSetId, rules, inherited, saving, onSave, onDelete }: {
  ruleSetId: number
  rules: Array<RuleDto>
  inherited: boolean
  saving: boolean
  onSave: (value: RuleWriteRequest, id?: number) => Promise<void>
  onDelete: (id: number) => void
}) {
  const { t } = useTranslation()
  const ordered = [...rules].sort((left, right) => RANK[left.config.kind] - RANK[right.config.kind] || left.sort_order - right.sort_order || left.id - right.id)
  return <Card>
    <CardHeader><CardTitle>{t("rules.entries.title")}</CardTitle><CardDescription>{t("rules.entries.orderDescription")}</CardDescription><CardAction><RuleDialog ruleSetId={ruleSetId} saving={saving} onSave={onSave} trigger={<Button size="sm" variant="outline">{t("rules.entries.add")}</Button>} /></CardAction></CardHeader>
    <CardContent className="flex flex-col gap-3">
      {ordered.length ? ordered.map((rule, index) => <div key={rule.id} className="flex flex-col gap-3 rounded-lg border p-3 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><Badge>{t("rules.entries.effectiveOrder", { order: index + 1 })}</Badge><Badge variant="outline">{t(`rules.kinds.${rule.config.kind}`)}</Badge>{inherited ? <Badge variant="secondary">{t("rules.values.inherited")}</Badge> : null}{!rule.enabled ? <Badge variant="secondary">{t("common.status.disabled")}</Badge> : null}</div><p className="mt-2 font-mono text-xs text-muted-foreground">{summary(rule, t)}</p><p className="mt-1 text-xs text-muted-foreground">{t("rules.entries.orderMeta", { rank: RANK[rule.config.kind], declared: rule.sort_order })}</p></div>
        <div className="flex items-center justify-end gap-2"><RuleDialog ruleSetId={ruleSetId} rule={rule} saving={saving} onSave={onSave} trigger={<Button size="icon-sm" variant="outline" aria-label={t("common.actions.edit")}><PencilIcon aria-hidden /></Button>} /><Button size="icon-sm" variant="ghost" aria-label={t("common.actions.delete")} onClick={() => onDelete(rule.id)}><Trash2Icon aria-hidden /></Button></div>
      </div>) : <Empty><EmptyHeader><EmptyTitle>{t("rules.entries.empty")}</EmptyTitle><EmptyDescription>{t("rules.entries.emptyDescription")}</EmptyDescription></EmptyHeader></Empty>}
    </CardContent>
  </Card>
}

function summary(rule: RuleDto, t: (key: string, options?: Record<string, unknown>) => string) {
  const config = rule.config
  if (config.kind === "system_text") return t("rules.entries.summary.system_text", { position: t(`rules.values.${config.position}`) })
  if (config.kind === "cache_breakpoint") return t("rules.entries.summary.cache_breakpoint", { target: t(`rules.values.${config.target}`), ttl: config.ttl ?? t("rules.values.inherited") })
  if (config.kind === "rewrite") return t("rules.entries.summary.rewrite", { action: t(`rules.values.${config.action}`), path: config.path })
  if (config.kind === "transform") return t("rules.entries.summary.transform", { phase: t(`rules.values.${config.phase}`), count: config.actions.length })
  return t("rules.entries.summary.header", { name: config.name, mode: t(`rules.values.${config.mode}`) })
}
