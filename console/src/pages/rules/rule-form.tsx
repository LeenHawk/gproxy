import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { RewriteRuleDto, RewriteRuleWrite } from "@/generated/sdk"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Switch } from "@/components/ui/switch"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export type SetChoice = { id: string; name: string; shared?: boolean }
import { kinds, ruleKind, type RuleKind } from "./rule-kind"
export function RuleSelect({ label, value, options, onChange, disabled }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void; disabled?: boolean }) {
  return <Field><FieldLabel>{label}</FieldLabel><Select value={value} onValueChange={onChange} disabled={disabled}><SelectTrigger aria-label={label}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{options.map(option => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
}

type Props = { original?: RewriteRuleDto; choices: SetChoice[]; defaultSetId: string; onClose: () => void; onSubmit: (write: RewriteRuleWrite) => void; pending: boolean; error: unknown }
export function RuleForm({ original, choices, defaultSetId, onClose, onSubmit, pending, error }: Props) {
  const { t } = useTranslation()
  const initialKind = original ? ruleKind(original) : "system_text"
  const [kind, setKind] = useState<RuleKind>(initialKind)
  const [setId, setSetId] = useState(original?.ruleSetId ?? defaultSetId)
  const [action, setAction] = useState(original?.action ?? "set")
  const [phase, setPhase] = useState(original?.phase ?? "request")
  const [target, setTarget] = useState(original?.target ?? "body")
  const [targetName, setTargetName] = useState(original?.targetName ?? "")
  const [paths, setPaths] = useState(original?.paths?.join("\n") ?? "")
  const [pattern, setPattern] = useState(original?.pattern ?? "")
  const [replacement, setReplacement] = useState(original?.replacement ?? "")
  const [config] = useState(() => original && ["system_text", "cache_breakpoint"].includes(original.action) ? JSON.parse(original.replacement) : {})
  const [dialect, setDialect] = useState<string>(config.dialect ?? "openai")
  const [text, setText] = useState<string>(config.text ?? "")
  const [position, setPosition] = useState<string>(config.position ?? "prepend")
  const [cacheTarget, setCacheTarget] = useState<string>(config.target ?? "message")
  const [cacheIndex, setCacheIndex] = useState<string>(config.index?.toString() ?? "")
  const [ttl, setTtl] = useState<string>(config.ttl ?? "default")
  const [modelFilter, setModelFilter] = useState(original?.filterModelPattern ?? "")
  const [headerFilter, setHeaderFilter] = useState(original?.filterHeaderPattern ?? "")
  const [eventFilter, setEventFilter] = useState(original?.filterEventPattern ?? "")
  const [operations, setOperations] = useState(original?.filterOperationKeys ? JSON.stringify(original.filterOperationKeys, null, 2) : "")
  const [enabled, setEnabled] = useState(original?.enabled ?? true)
  const [validation, setValidation] = useState<string | null>(null)
  const options = (values: readonly string[], prefix = "rules.options") => values.map(value => ({ value, label: prefix === "rules.protocols" ? t(`rules.protocols.${value}`) : prefix === "rules.types" ? t(`rules.types.${value}`) : t(`rules.options.${value}`) }))
  const select = (label: string, value: string, values: readonly string[], onChange: (v: string) => void, prefix?: string) => <RuleSelect label={label} value={value} options={options(values, prefix)} onChange={onChange} />
  const input = (label: string, value: string, onChange: (value: string) => void, multiline = false, required = false) => <Field><FieldLabel>{label}</FieldLabel>{multiline ? <Textarea aria-label={label} value={value} required={required} onChange={e => onChange(e.target.value)} /> : <Input aria-label={label} value={value} required={required} onChange={e => onChange(e.target.value)} />}</Field>
  const semantic = kind === "system_text" || kind === "cache_breakpoint"
  const submit = () => {
    try {
      const operationKeys: unknown = operations.trim() ? JSON.parse(operations) : null
      if (operationKeys !== null && (!Array.isArray(operationKeys) || operationKeys.some(v => !v || typeof v.operation !== "string" || typeof v.dialect !== "string"))) throw new Error(t("rules.invalidOperations"))
      if (kind === "rewrite" && action !== "delete") {
        const value = JSON.parse(replacement)
        if (action === "merge" && (!value || typeof value !== "object" || Array.isArray(value))) throw new Error(t("rules.mergeObject"))
      }
      const index = cacheIndex.trim() ? Number(cacheIndex) : null
      if (kind === "cache_breakpoint" && index !== null && (!Number.isSafeInteger(index) || index === 0)) throw new Error(t("rules.invalidIndex"))
      const bodyPaths = paths.split("\n").map(p => p.trim()).filter(Boolean)
      if (kind === "rewrite" && bodyPaths.length === 0) throw new Error(t("rules.pathsRequired"))
      onSubmit({
        id: original?.id ?? null, ruleSetId: setId,
        action: semantic ? kind : kind === "rewrite" || kind === "header" ? action : "replace",
        phase: semantic || kind === "header" || (kind === "transform" && target === "query") ? "request" : phase,
        target: kind === "header" ? "header" : kind === "transform" ? target : "body",
        targetName: kind === "header" || (kind === "transform" && target !== "body") ? targetName : null,
        paths: !semantic && kind !== "header" && (kind === "rewrite" || target === "body") && bodyPaths.length ? bodyPaths : null,
        pattern: kind === "transform" ? pattern : "",
        replacement: kind === "system_text" ? JSON.stringify({ dialect, text, position }) : kind === "cache_breakpoint" ? JSON.stringify({ dialect, target: cacheTarget, index, ttl: ttl === "default" ? null : ttl }) : kind === "rewrite" && action === "delete" ? "" : replacement,
        filterModelPattern: modelFilter || null, filterHeaderPattern: headerFilter || null,
        filterEventPattern: !semantic && kind !== "header" && target === "body" ? eventFilter || null : null,
        filterOperationKeys: operationKeys as RewriteRuleWrite["filterOperationKeys"],
        sortOrder: original?.sortOrder ?? null, enabled,
      })
      setValidation(null)
    } catch (error) { setValidation(error instanceof Error ? error.message : String(error)) }
  }
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent className="sm:max-w-2xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t(original ? "edit.rules" : "create.rules")}</DialogTitle></DialogHeader>
    <form className="flex min-h-0 flex-col" onSubmit={e => { e.preventDefault(); submit() }}>
      <DialogBody><fieldset disabled={pending} className="min-w-0"><FieldGroup>
        <RuleSelect label={t("fields.ruleSetId")} value={setId} options={choices.map(set => ({ value: set.id, label: set.name }))} onChange={setSetId} disabled={!!original} />
        {choices.find(set => set.id === setId)?.shared ? <FieldDescription>{t("rules.sharedWarning")}</FieldDescription> : null}
        <RuleSelect label={t("rules.ruleType")} value={kind} options={options(kinds, "rules.types")} onChange={value => { const next = value as RuleKind; setKind(next); setTarget("body"); setPhase("request"); setEventFilter(""); setAction(next === "header" ? "header_set" : "set"); setValidation(null); if (next === "cache_breakpoint" && dialect === "gemini") setDialect("claude") }} />
        <FieldDescription>{t(`rules.templateHelp.${kind}`)}</FieldDescription>
        {semantic ? select(t("fields.dialect"), dialect, kind === "system_text" ? ["openai", "openai_chat", "claude", "gemini", "openai_responses_websocket"] : ["openai", "openai_chat", "claude", "openai_responses_websocket"], value => { setDialect(value); setTtl("default"); if (value !== "claude" && cacheTarget === "tools") setCacheTarget("message") }, "rules.protocols") : null}
        {kind === "system_text" ? <>{select(t("rules.position"), position, ["prepend", "append"], setPosition)}{input(t("rules.text"), text, setText, true, true)}</> : null}
        {kind === "cache_breakpoint" ? <>
          {select(t("rules.cacheTarget"), cacheTarget, dialect === "claude" ? ["global", "system", "message", "tools"] : ["global", "system", "message"], setCacheTarget)}
          {cacheTarget !== "global" ? <>{input(t("rules.cacheIndex"), cacheIndex, setCacheIndex)}<FieldDescription>{t("rules.cacheIndexHelp")}</FieldDescription></> : null}
          {select(t("rules.ttl"), ttl, dialect === "claude" ? ["default", "5m", "1h"] : ["default", "30m"], setTtl)}
        </> : null}
        {kind === "rewrite" ? <>{select(t("fields.action"), action, ["set", "delete", "merge"], setAction)}{input(t("fields.paths"), paths, setPaths, true, true)}{action !== "delete" ? input(t("rules.jsonValue"), replacement, setReplacement, true, true) : null}</> : null}
        {kind === "header" ? <>{select(t("fields.action"), action, ["header_set", "header_merge"], setAction)}{input(t("fields.targetName"), targetName, setTargetName, false, true)}{input(t("fields.replacement"), replacement, setReplacement)}</> : null}
        {kind === "transform" ? <>
          {select(t("fields.target"), target, ["body", "header", "query"], setTarget)}
          {target === "body" ? input(t("fields.paths"), paths, setPaths, true) : input(t("fields.targetName"), targetName, setTargetName, false, true)}
          {input(t("fields.pattern"), pattern, setPattern, false, true)}{input(t("fields.replacement"), replacement, setReplacement, true)}
        </> : null}
        {!semantic && kind !== "header" && !(kind === "transform" && target === "query") ? select(t("fields.phase"), phase, ["request", "response", "both"], setPhase) : null}
        <details><summary className="cursor-pointer text-sm">{t("rules.filters")}</summary><FieldGroup className="mt-3">
          {input(t("fields.filterModelPattern"), modelFilter, setModelFilter)}{input(t("fields.filterHeaderPattern"), headerFilter, setHeaderFilter)}
          {!semantic && kind !== "header" && target === "body" ? input(t("fields.filterEventPattern"), eventFilter, setEventFilter) : null}
          {input(t("fields.filterOperationKeys"), operations, setOperations, true)}
        </FieldGroup></details>
        <Field orientation="horizontal"><FieldLabel htmlFor="rule-enabled">{t("fields.enabled")}</FieldLabel><Switch id="rule-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
      </FieldGroup></fieldset>{validation || error ? <ErrorNotice error={validation ?? error} /> : null}</DialogBody>
      <DialogFooter><Button type="submit" disabled={pending || !setId}>{t("actions.save")}</Button></DialogFooter>
    </form>
  </DialogContent></Dialog>
}
