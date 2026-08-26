import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { RuleConfigDto } from "@/generated/RuleConfigDto"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleWriteRequest } from "@/generated/RuleWriteRequest"
import { configFromDraft, ruleDraft, type RuleDraft } from "./rule-draft"
import { RuleConfigFields } from "./rule-config-fields"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

const KINDS: Array<RuleConfigDto["kind"]> = ["system_text", "cache_breakpoint", "rewrite", "transform", "header"]

export function RuleDialog({ ruleSetId, rule, trigger, saving, onSave }: {
  ruleSetId: number
  rule?: RuleDto
  trigger: ReactElement
  saving: boolean
  onSave: (value: RuleWriteRequest, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<RuleDraft>(() => ruleDraft(rule))
  const [model, setModel] = useState(rule?.filter_model_pattern ?? "")
  const [operations, setOperations] = useState(rule?.filter_operations?.join(", ") ?? "")
  const [headers, setHeaders] = useState(rule?.filter_header_pattern ?? "")
  const [sortOrder, setSortOrder] = useState(String(rule?.sort_order ?? 0))
  const [enabled, setEnabled] = useState(rule?.enabled ?? true)
  const [error, setError] = useState("")
  const reset = () => {
    setDraft(ruleDraft(rule))
    setModel(rule?.filter_model_pattern ?? "")
    setOperations(rule?.filter_operations?.join(", ") ?? "")
    setHeaders(rule?.filter_header_pattern ?? "")
    setSortOrder(String(rule?.sort_order ?? 0))
    setEnabled(rule?.enabled ?? true)
    setError("")
  }
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    try {
      const config = configFromDraft(draft)
      await onSave({
        rule_set_id: ruleSetId,
        config,
        filter_model_pattern: model.trim() || null,
        filter_operations: operationFilters(operations),
        filter_header_pattern: headers.trim() || null,
        sort_order: Number(sortOrder),
        enabled,
      }, rule?.id)
      setOpen(false)
    } catch {
      setError(t("rules.validation.config"))
    }
  }
  return (
    <Dialog open={open} onOpenChange={(next) => { if (next) reset(); setOpen(next) }}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-4xl" showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
          <DialogHeader><DialogTitle>{t(rule ? "rules.entries.edit" : "rules.entries.add")}</DialogTitle></DialogHeader>
          <DialogBody><FieldGroup>
            {error ? <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert> : null}
            <Field><FieldLabel htmlFor="rule-kind">{t("rules.fields.kind")}</FieldLabel><Select name="rule-kind" value={draft.kind} onValueChange={(kind) => setDraft({ ...ruleDraft(), kind: kind as RuleConfigDto["kind"] })}><SelectTrigger id="rule-kind" className="w-full"><SelectValue /></SelectTrigger><SelectContent>{KINDS.map((kind) => <SelectItem key={kind} value={kind}>{t(`rules.kinds.${kind}`)}</SelectItem>)}</SelectContent></Select></Field>
            <RuleConfigFields draft={draft} onChange={setDraft} />
            <Field><FieldLabel htmlFor="rule-model-filter">{t("rules.filters.model")}</FieldLabel><Input id="rule-model-filter" className="font-mono" value={model} placeholder={t("rules.placeholders.allModels")} onChange={(event) => setModel(event.target.value)} /><FieldDescription>{t("rules.filters.modelHelp")}</FieldDescription></Field>
            <Field><FieldLabel htmlFor="rule-operation-filter">{t("rules.filters.operations")}</FieldLabel><Input id="rule-operation-filter" className="font-mono" value={operations} placeholder={t("rules.placeholders.allOperations")} onChange={(event) => setOperations(event.target.value)} /><FieldDescription>{t("rules.filters.operationsHelp")}</FieldDescription></Field>
            <Field><FieldLabel htmlFor="rule-header-filter">{t("rules.filters.headers")}</FieldLabel><Input id="rule-header-filter" className="font-mono" value={headers} placeholder={t("rules.placeholders.allHeaders")} onChange={(event) => setHeaders(event.target.value)} /><FieldDescription>{t("rules.filters.headersHelp")}</FieldDescription></Field>
            <div className="grid gap-4 sm:grid-cols-2"><Field><FieldLabel htmlFor="rule-sort-order">{t("rules.fields.declaredOrder")}</FieldLabel><Input id="rule-sort-order" type="number" value={sortOrder} onChange={(event) => setSortOrder(event.target.value)} /></Field><Field orientation="horizontal"><FieldLabel htmlFor="rule-enabled">{t("rules.fields.enabled")}</FieldLabel><Switch id="rule-enabled" name="rule-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field></div>
          </FieldGroup></DialogBody>
          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function operationFilters(value: string) {
  const operations = value.split(",").map((item) => item.trim()).filter(Boolean)
  return operations.length ? operations : null
}
