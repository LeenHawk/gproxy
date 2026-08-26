import type { ReactElement } from "react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { RuleSetWriteRequest } from "@/generated/RuleSetWriteRequest"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

export function RuleSetDialog({ ruleSet, trigger, saving, onSave }: {
  ruleSet?: RuleSetDto
  trigger: ReactElement
  saving: boolean
  onSave: (value: RuleSetWriteRequest, id?: number) => Promise<void>
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState(ruleSet?.name ?? "")
  const [description, setDescription] = useState(ruleSet?.description ?? "")
  const [enabled, setEnabled] = useState(ruleSet?.enabled ?? true)
  const reset = () => {
    setName(ruleSet?.name ?? "")
    setDescription(ruleSet?.description ?? "")
    setEnabled(ruleSet?.enabled ?? true)
  }
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    await onSave({ name: name.trim(), description: description.trim() || null, enabled }, ruleSet?.id)
    setOpen(false)
  }
  return (
    <Dialog open={open} onOpenChange={(next) => { if (next) reset(); setOpen(next) }}>
      <DialogTrigger asChild>{trigger}</DialogTrigger>
      <DialogContent showCloseButton={false}>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => void submit(event)}>
          <DialogHeader><DialogTitle>{t(ruleSet ? "rules.sets.edit" : "rules.sets.add")}</DialogTitle></DialogHeader>
          <DialogBody><FieldGroup>
            <Field><FieldLabel htmlFor="rule-set-name">{t("rules.fields.name")}</FieldLabel><Input id="rule-set-name" required value={name} onChange={(event) => setName(event.target.value)} /></Field>
            <Field><FieldLabel htmlFor="rule-set-description">{t("rules.fields.description")}</FieldLabel><Textarea id="rule-set-description" value={description} onChange={(event) => setDescription(event.target.value)} /></Field>
            <Field orientation="horizontal"><FieldLabel htmlFor="rule-set-enabled">{t("rules.fields.enabled")}</FieldLabel><Switch id="rule-set-enabled" name="rule-set-enabled" checked={enabled} onCheckedChange={setEnabled} /></Field>
          </FieldGroup></DialogBody>
          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose><Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : "common.actions.save")}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
