import { PlusIcon, Trash2Icon } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { RuleDraft } from "./rule-draft"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"

export function RuleConfigFields({ draft, onChange }: { draft: RuleDraft; onChange: (draft: RuleDraft) => void }) {
  const { t } = useTranslation()
  const set = <K extends keyof RuleDraft>(key: K, value: RuleDraft[K]) => onChange({ ...draft, [key]: value })
  const locateType = draft.locateType || "path"
  if (draft.kind === "system_text") return <>
    <Field><FieldLabel htmlFor="rule-text">{t("rules.fields.text")}</FieldLabel><Textarea id="rule-text" required value={draft.text} onChange={(event) => set("text", event.target.value)} /></Field>
    <Choice id="rule-position" label={t("rules.fields.position")} value={draft.position} options={["prepend", "append"]} onValue={(value) => set("position", value as RuleDraft["position"])} />
  </>
  if (draft.kind === "cache_breakpoint") return <>
    <Choice id="cache-target" label={t("rules.fields.target")} value={draft.cacheTarget} options={["top_level", "system", "tools", "message"]} onValue={(value) => set("cacheTarget", value)} />
    <Field><FieldLabel htmlFor="cache-index">{t("rules.fields.index")}</FieldLabel><Input id="cache-index" type="number" placeholder={t("rules.placeholders.lastBlock")} value={draft.cacheIndex} onChange={(event) => set("cacheIndex", event.target.value)} /></Field>
    <Choice id="cache-ttl" label={t("rules.fields.ttl")} value={draft.ttl || "inherited"} options={["inherited", "5m", "30m", "1h"]} onValue={(value) => set("ttl", value === "inherited" ? "" : value)} inherited />
  </>
  if (draft.kind === "rewrite") return <>
    <Field><FieldLabel htmlFor="rewrite-path">{t("rules.fields.path")}</FieldLabel><Input id="rewrite-path" className="font-mono" required value={draft.path} onChange={(event) => set("path", event.target.value)} /></Field>
    <Choice id="rewrite-action" label={t("rules.fields.action")} value={draft.rewriteAction} options={["set", "delete", "merge"]} onValue={(value) => set("rewriteAction", value as RuleDraft["rewriteAction"])} />
    {draft.rewriteAction === "delete" ? null : <Field><FieldLabel htmlFor="rewrite-value">{t("rules.fields.value")}</FieldLabel><Textarea id="rewrite-value" className="font-mono" required value={draft.rewriteValue} onChange={(event) => set("rewriteValue", event.target.value)} /><FieldDescription>{t("rules.help.jsonValue")}</FieldDescription></Field>}
  </>
  if (draft.kind === "header") return <>
    <Field><FieldLabel htmlFor="header-name">{t("rules.fields.headerName")}</FieldLabel><Input id="header-name" className="font-mono" required value={draft.headerName} onChange={(event) => set("headerName", event.target.value)} /></Field>
    <Field><FieldLabel htmlFor="header-value">{t("rules.fields.headerValue")}</FieldLabel><Input id="header-value" required value={draft.headerValue} onChange={(event) => set("headerValue", event.target.value)} /></Field>
    <Choice id="header-mode" label={t("rules.fields.mergeMode")} value={draft.headerMode} options={["override", "merge"]} onValue={(value) => set("headerMode", value as RuleDraft["headerMode"])} />
  </>
  return <>
    <div className="flex items-center justify-between gap-3"><FieldDescription>{t("rules.transform.rawHint")}</FieldDescription><Button type="button" size="sm" variant="outline" onClick={() => onChange({ ...draft, phase: "request", locateType: "paths", locateValue: "messages.*.content.*.text\ninput.*.content.*.text", actions: [{ op: "replace_text", from: "", pattern: "", with: "" }] })}>{t("rules.transformTemplate.fillFrom")}</Button></div>
    <Choice id="transform-phase" label={t("rules.fields.phase")} value={draft.phase} options={["request", "response", "both"]} onValue={(value) => set("phase", value as RuleDraft["phase"])} />
    <Choice id="transform-locate" label={t("rules.fields.locate")} value={locateType} options={["path", "paths", "match"]} onValue={(value) => set("locateType", value as RuleDraft["locateType"])} />
    <Field><FieldLabel htmlFor="transform-locate-value">{t(`rules.fields.locateValue.${locateType}`)}</FieldLabel>{locateType === "paths" ? <Textarea id="transform-locate-value" className="font-mono" required value={draft.locateValue} onChange={(event) => set("locateValue", event.target.value)} /> : <Input id="transform-locate-value" className="font-mono" required value={draft.locateValue} onChange={(event) => set("locateValue", event.target.value)} />}</Field>
    <FieldSet><FieldLegend>{t("rules.fields.actions")}</FieldLegend>
      {draft.actions.map((action, index) => <div key={index} className="grid gap-3 rounded-lg border p-3 sm:grid-cols-2">
        <Choice id={`transform-action-${index}`} label={t("rules.fields.action")} value={action.op} options={["replace_text", "replace_regex"]} onValue={(value) => set("actions", draft.actions.map((item, itemIndex) => itemIndex === index ? { ...item, op: value as typeof item.op } : item))} />
        {action.op === "replace_text" ? <Field><FieldLabel htmlFor={`transform-from-${index}`}>{t("rules.fields.from")}</FieldLabel><Input id={`transform-from-${index}`} value={action.from} placeholder={t("rules.placeholders.anyText")} onChange={(event) => set("actions", draft.actions.map((item, itemIndex) => itemIndex === index ? { ...item, from: event.target.value } : item))} /></Field> : <Field><FieldLabel htmlFor={`transform-pattern-${index}`}>{t("rules.fields.pattern")}</FieldLabel><Input id={`transform-pattern-${index}`} className="font-mono" required value={action.pattern} onChange={(event) => set("actions", draft.actions.map((item, itemIndex) => itemIndex === index ? { ...item, pattern: event.target.value } : item))} /></Field>}
        <Field><FieldLabel htmlFor={`transform-with-${index}`}>{t("rules.fields.with")}</FieldLabel><Input id={`transform-with-${index}`} required value={action.with} onChange={(event) => set("actions", draft.actions.map((item, itemIndex) => itemIndex === index ? { ...item, with: event.target.value } : item))} /></Field>
        <div className="flex items-end justify-end"><Button type="button" size="icon-sm" variant="ghost" aria-label={t("rules.actions.removeAction")} disabled={draft.actions.length === 1} onClick={() => set("actions", draft.actions.filter((_, itemIndex) => itemIndex !== index))}><Trash2Icon aria-hidden /></Button></div>
      </div>)}
      <Button type="button" size="sm" variant="outline" onClick={() => set("actions", [...draft.actions, { op: "replace_text", from: "", pattern: "", with: "" }])}><PlusIcon data-icon="inline-start" />{t("rules.actions.addAction")}</Button>
    </FieldSet>
    <Field><FieldLabel htmlFor="transform-limit">{t("rules.fields.limit")}</FieldLabel><Input id="transform-limit" type="number" min={1} placeholder={t("rules.placeholders.unlimited")} value={draft.limit} onChange={(event) => set("limit", event.target.value)} /></Field>
  </>
}

function Choice({ id, label, value, options, inherited, onValue }: { id: string; label: string; value: string; options: Array<string>; inherited?: boolean; onValue: (value: string) => void }) {
  const { t } = useTranslation()
  return <Field><FieldLabel htmlFor={id}>{label}</FieldLabel><Select name={id} value={value} onValueChange={onValue}><SelectTrigger id={id} className="w-full"><SelectValue /></SelectTrigger><SelectContent>{options.map((option) => <SelectItem key={option} value={option}>{inherited && option === "inherited" ? t("rules.values.inherited") : t(`rules.values.${option}`)}</SelectItem>)}</SelectContent></Select></Field>
}
