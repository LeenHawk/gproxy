import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import type { IdResponse } from "@/generated/IdResponse"
import type { RuleSetDto } from "@/generated/RuleSetDto"
import type { RuleSetWriteRequest } from "@/generated/RuleSetWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

export function RuleSetForm({ ruleSet, saving, onSave, onSaved }: {
  ruleSet?: RuleSetDto
  saving: boolean
  onSave: (value: RuleSetWriteRequest, id?: number) => Promise<IdResponse | undefined>
  onSaved?: (result: IdResponse | undefined) => void
}) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState(ruleSet?.name ?? "")
  const [description, setDescription] = useState(ruleSet?.description ?? "")
  const [enabled, setEnabled] = useState(ruleSet?.enabled ?? true)
  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    try {
      const result = await onSave({ name: name.trim(), description: description.trim() || null, enabled }, ruleSet?.id)
      onSaved?.(result)
    } catch {
      return
    }
  }

  return <form className="flex max-w-xl flex-col gap-4" onSubmit={(event) => void submit(event)}>
    <FieldGroup>
      <Field><FieldLabel htmlFor={`${id}-name`}>{t("rules.fields.name")}</FieldLabel><Input id={`${id}-name`} required value={name} onChange={(event) => setName(event.target.value)} /></Field>
      <Field><FieldLabel htmlFor={`${id}-description`}>{t("rules.fields.description")}</FieldLabel><Textarea id={`${id}-description`} value={description} onChange={(event) => setDescription(event.target.value)} /></Field>
      <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-enabled`}>{t("rules.fields.enabled")}</FieldLabel><Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} /></Field>
    </FieldGroup>
    <Button className="self-start" disabled={saving || !name.trim()}>{t(saving ? "common.actions.saving" : ruleSet ? "common.actions.save" : "common.actions.create")}</Button>
  </form>
}
