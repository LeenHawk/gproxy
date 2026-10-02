import { FilterFields } from "./filter-fields"
import { useQuery } from "@tanstack/react-query"
import { ruleSets } from "@/api/routing-rules"
import { optionSource } from "@/api/options"
import { SearchableSelect } from "@/components/searchable-select"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import type { RewriteRuleDto, RewriteRuleWrite } from "@/generated/sdk"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Switch } from "@/components/ui/switch"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { parseRuleJson, ruleJson } from "./rule-json"

export type SetChoice = { id: string; name: string; shared?: boolean }
import { kinds, ruleKind, type RuleKind } from "./rule-kind"
export function RuleSelect({ label, value, options, onChange, disabled }: { label: string; value: string; options: { value: string; label: string }[]; onChange: (value: string) => void; disabled?: boolean }) {
  return <Field><FieldLabel>{label}</FieldLabel><Select value={value} onValueChange={onChange} disabled={disabled}><SelectTrigger aria-label={label}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{options.map(option => <SelectItem key={option.value} value={option.value}>{option.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
}

export function RuleSetSelect({ value, choices, remote, onChange, disabled }: { value: string; choices: SetChoice[]; remote?: boolean; onChange: (value: string) => void; disabled?: boolean }) {
  const { t } = useTranslation(), id = useId()
  const known = choices.find(set => set.id === value)
  const selected = useQuery({ queryKey: ["admin", "/rule-sets", "detail", value], queryFn: () => ruleSets.get(value), enabled: !!remote && !!value && !known })
  const options = choices.map(set => ({ value: set.id, label: set.name }))
  if (selected.data && !known) options.push({ value, label: selected.data.name })
  return <Field><FieldLabel htmlFor={id}>{t("fields.ruleSetId")}</FieldLabel>
    <SearchableSelect aria-describedby={`${id}-description-0`} id={id} label={t("fields.ruleSetId")} value={value} options={options} source={remote ? optionSource(ruleSets, row => ({ value: row.id, label: row.name })) : undefined} onChange={onChange} disabled={disabled} />
    {known?.shared || (selected.data?.providerCount ?? 0) > 0 ? <FieldDescription id={`${id}-description-0`}>{t("rules.sharedWarning")}</FieldDescription> : null}
    {selected.error ? <ErrorNotice error={selected.error} /> : null}
  </Field>
}

type Props = { providerId?: string; remoteSets?: boolean; original?: RewriteRuleDto; choices: SetChoice[]; defaultSetId: string; onClose: () => void; onSubmit: (write: RewriteRuleWrite) => void; pending: boolean; error: unknown }
export function RuleForm({ providerId, original, choices, remoteSets, defaultSetId, onClose, onSubmit, pending, error }: Props) {
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
  const [config, setConfig] = useState(() => original && ["system_text", "cache_breakpoint"].includes(original.action) ? JSON.parse(original.replacement) : {})
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
  const [mode, setMode] = useState("form")
  const [jsonText, setJsonText] = useState("")
  const jsonId = useId()
  const options = (values: readonly string[], prefix = "rules.options") => values.map(value => ({ value, label: prefix === "rules.protocols" ? t(`rules.protocols.${value}`) : prefix === "rules.types" ? t(`rules.types.${value}`) : t(`rules.options.${value}`) }))
  const select = (label: string, value: string, values: readonly string[], onChange: (v: string) => void, prefix?: string) => <RuleSelect label={label} value={value} options={options(values, prefix)} onChange={onChange} />
  const input = (label: string, value: string, onChange: (value: string) => void, multiline = false, required = false) => <Field><FieldLabel>{label}</FieldLabel>{multiline ? <Textarea aria-label={label} value={value} required={required} onChange={e => onChange(e.target.value)} /> : <Input aria-label={label} value={value} required={required} onChange={e => onChange(e.target.value)} />}</Field>
  const semantic = kind === "system_text" || kind === "cache_breakpoint"
  const formWrite = (validate = true): RewriteRuleWrite => {
    const operationKeys: unknown = operations.trim() ? JSON.parse(operations) : null
    if (operationKeys !== null && (!Array.isArray(operationKeys) || operationKeys.some(v => !v || typeof v.operation !== "string" || typeof v.dialect !== "string"))) throw new Error(t("rules.invalidOperations"))
    if (validate && kind === "rewrite" && action !== "delete") {
      const value = JSON.parse(replacement)
      if (action === "merge" && (!value || typeof value !== "object" || Array.isArray(value))) throw new Error(t("rules.mergeObject"))
    }
    const index = cacheIndex.trim() ? Number(cacheIndex) : null
    if (kind === "cache_breakpoint" && index !== null && (!Number.isSafeInteger(index) || index === 0)) throw new Error(t("rules.invalidIndex"))
    const bodyPaths = paths.split("\n").map(p => p.trim()).filter(Boolean)
    if (validate && kind === "rewrite" && bodyPaths.length === 0) throw new Error(t("rules.pathsRequired"))
    return {
      id: original?.id ?? null, ruleSetId: setId,
      action: semantic ? kind : kind === "rewrite" || kind === "header" ? action : "replace",
      phase: semantic || (kind === "transform" && target === "query") ? "request" : phase,
      target: kind === "header" ? "header" : kind === "transform" ? target : "body",
      targetName: kind === "header" || (kind === "transform" && target !== "body") ? targetName : null,
      paths: !semantic && kind !== "header" && (kind === "rewrite" || target === "body") && bodyPaths.length ? bodyPaths : null,
      pattern: kind === "transform" ? pattern : "",
      replacement: kind === "system_text" ? JSON.stringify({ ...config, dialect, text, position }) : kind === "cache_breakpoint" ? JSON.stringify({ ...config, dialect, target: cacheTarget, index, ttl: ttl === "default" ? null : ttl }) : kind === "rewrite" && action === "delete" ? "" : replacement,
      filterModelPattern: modelFilter || null, filterHeaderPattern: headerFilter || null,
      filterEventPattern: !semantic && kind !== "header" && target === "body" ? eventFilter || null : null,
      filterOperationKeys: operationKeys as RewriteRuleWrite["filterOperationKeys"],
      sortOrder: original?.sortOrder ?? null, enabled,
    }
  }
  const parseJson = () => parseRuleJson(jsonText, field => t("rules.invalidJsonField", { field }))
  const formatJson = () => {
    try { setJsonText(JSON.stringify(JSON.parse(jsonText), null, 2)); setValidation(null) }
    catch (error) { setValidation(error instanceof Error ? error.message : String(error)) }
  }
  const changeMode = (next: string) => {
    if (!next || next === mode) return
    try {
      if (next === "json") setJsonText(JSON.stringify(ruleJson(formWrite(false)), null, 2))
      else {
        const write = parseJson()
        const nextAction = write.action ?? "replace", nextTarget = write.target ?? "body"
        const nextKind = ruleKind({ action: nextAction, target: nextTarget })
        const content = nextKind === "system_text" || nextKind === "cache_breakpoint" ? JSON.parse(write.replacement) : {}
        if (!content || typeof content !== "object" || Array.isArray(content)) throw new Error(t("rules.invalidJsonField", { field: "replacement" }))
        for (const field of ["dialect", "text", "position", "target", "ttl"]) {
          if (content[field] != null && typeof content[field] !== "string") throw new Error(t("rules.invalidJsonField", { field: `replacement.${field}` }))
        }
        if (content.index != null && !Number.isSafeInteger(content.index)) throw new Error(t("rules.invalidIndex"))
        setKind(nextKind); setAction(nextAction); setPhase(write.phase ?? "request"); setTarget(nextTarget)
        setTargetName(write.targetName ?? ""); setPaths(write.paths?.join("\n") ?? "")
        setPattern(write.pattern); setReplacement(write.replacement); setConfig(content)
        setDialect(content.dialect ?? "openai"); setText(content.text ?? ""); setPosition(content.position ?? "prepend")
        setCacheTarget(content.target ?? "message"); setCacheIndex(content.index?.toString() ?? ""); setTtl(content.ttl ?? "default")
        setModelFilter(write.filterModelPattern ?? ""); setHeaderFilter(write.filterHeaderPattern ?? ""); setEventFilter(write.filterEventPattern ?? "")
        setOperations(write.filterOperationKeys ? JSON.stringify(write.filterOperationKeys, null, 2) : ""); setEnabled(write.enabled ?? true)
      }
      setMode(next)
      setValidation(null)
    } catch (error) { setValidation(error instanceof Error ? error.message : String(error)) }
  }
  const submit = () => {
    try {
      onSubmit(mode === "json" ? { ...parseJson(), id: original?.id ?? null, ruleSetId: setId, sortOrder: original?.sortOrder ?? null } : formWrite())
      setValidation(null)
    } catch (error) { setValidation(error instanceof Error ? error.message : String(error)) }
  }
  return <Dialog open onOpenChange={open => { if (!open && !pending) onClose() }}><DialogContent className="sm:max-w-2xl" closeLabel={t("actions.close")} aria-describedby={undefined}>
    <DialogHeader><DialogTitle>{t(original ? "edit.rules" : "create.rules")}</DialogTitle></DialogHeader>
    <form className="flex min-h-0 flex-col" onSubmit={e => { e.preventDefault(); submit() }}>
      <DialogBody><fieldset disabled={pending} className="min-w-0"><FieldGroup>
        <RuleSetSelect value={setId} choices={choices} remote={remoteSets} onChange={setSetId} disabled={!!original} />
        <ToggleGroup type="single" variant="outline" value={mode} onValueChange={changeMode} aria-label={t("rules.editorMode")} disabled={pending}>
          <ToggleGroupItem value="form">{t("rules.formMode")}</ToggleGroupItem><ToggleGroupItem value="json">{t("rules.jsonMode")}</ToggleGroupItem>
        </ToggleGroup>
        {mode === "json" ? <Field className="sm:col-span-2">
          <FieldLabel htmlFor={jsonId}>{t("rules.ruleJson")}</FieldLabel>
          <FieldDescription id={`${jsonId}-description-0`}>{t("rules.jsonHelp")}</FieldDescription>
          <Textarea aria-describedby={`${jsonId}-description-0`} id={jsonId} value={jsonText} onChange={event => { setJsonText(event.target.value); setValidation(null) }} rows={18} className="font-mono" spellCheck={false} autoComplete="off" aria-invalid={!!validation} />
          <Button type="button" variant="outline" size="sm" className="self-start" onClick={formatJson}>{t("rules.formatJson")}</Button>
        </Field> : <>
          <RuleSelect label={t("rules.ruleType")} value={kind} options={options(kinds, "rules.types")} onChange={value => { const next = value as RuleKind; setKind(next); setConfig({}); setTarget("body"); setPhase("request"); setEventFilter(""); setAction(next === "header" ? "header_set" : "set"); setValidation(null); if (next === "cache_breakpoint" && dialect === "gemini") setDialect("claude") }} />
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
        {!semantic && !(kind === "transform" && target === "query") ? select(t("fields.phase"), phase, ["request", "response", "both"], setPhase) : null}
        <details data-field-span="full"><summary className="cursor-pointer text-sm">{t("rules.filters")}</summary><FieldGroup className="mt-3">
          <FilterFields providerId={providerId} ruleSetId={setId} model={modelFilter} onModel={setModelFilter} operations={operations} onOperations={setOperations} headers={headerFilter} onHeaders={setHeaderFilter} />
          {!semantic && kind !== "header" && target === "body" ? input(t("fields.filterEventPattern"), eventFilter, setEventFilter) : null}
        </FieldGroup></details>
        <Field orientation="horizontal"><FieldLabel htmlFor="rule-enabled">{t("fields.enabled")}</FieldLabel><Switch id="rule-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
        </>}
      </FieldGroup></fieldset>{validation || error ? <ErrorNotice error={validation ?? error} /> : null}</DialogBody>
      <DialogFooter><Button type="submit" disabled={pending || !setId}>{t("actions.save")}</Button></DialogFooter>
    </form>
  </DialogContent></Dialog>
}
