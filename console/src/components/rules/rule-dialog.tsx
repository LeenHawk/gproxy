import { FilterFields } from "./filter-fields"
import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { RuleDto } from "@/generated/RuleDto"
import type { RuleWriteRequest } from "@/generated/RuleWriteRequest"
import { configFromDraft, ruleDraft, type RuleDraft } from "./rule-draft"
import { RuleConfigFields } from "./rule-config-fields"
import { RuleKindPicker } from "./rule-kind-picker"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

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
    const invalid = validationKey(draft, sortOrder)
    if (invalid) {
      setError(t(`rules.validation.${invalid}`))
      return
    }
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
      setError(t("rules.validation.configInvalid"))
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
            <FieldSet data-field-span="full"><FieldLegend variant="label">{t("rules.fields.kind")}</FieldLegend><RuleKindPicker value={draft.kind} onChange={(kind) => setDraft({ ...ruleDraft(), kind })} /></FieldSet>
            <RuleConfigFields draft={draft} onChange={setDraft} />
            {open ? <FilterFields model={model} onModel={setModel} operations={operations} onOperations={setOperations} headers={headers} onHeaders={setHeaders} /> : null}
            <div className="grid gap-4 sm:grid-cols-2" data-field-span="full"><Field><FieldLabel htmlFor="rule-sort-order">{t("rules.fields.declaredOrder")}</FieldLabel><Input id="rule-sort-order" type="number" value={sortOrder} onChange={(event) => setSortOrder(event.target.value)} /></Field><Field orientation="horizontal"><FieldLabel htmlFor="rule-enabled">{t("rules.fields.enabled")}</FieldLabel><Switch id="rule-enabled" name="rule-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field></div>
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

function validationKey(draft: RuleDraft, sortOrder: string) {
  if (!Number.isFinite(Number(sortOrder))) return "sortOrderRequired"
  if (draft.kind === "system_text" && !draft.text.trim()) return "configTextRequired"
  if (draft.kind === "cache_breakpoint" && !draft.cacheTarget) return "configTargetRequired"
  if (draft.kind === "cache_breakpoint" && Number(draft.cacheIndex) === 0) return "cacheIndexZero"
  if (draft.kind === "rewrite" && !draft.path.trim()) return "configPathRequired"
  if (draft.kind === "header" && !draft.headerName.trim()) return "configHeaderNameRequired"
  if (draft.kind === "transform" && (!draft.locateValue.trim() || draft.actions.length === 0)) return "configTransformRequired"
  return null
}
