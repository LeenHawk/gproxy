import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { OrganizationWriteRequest } from "@/generated/OrganizationWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function OrganizationProfileForm({ organization, pending, onSave }: {
  organization: OrganizationDto
  pending: boolean
  onSave: (value: OrganizationWriteRequest) => Promise<void>
}) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState(organization.name)
  const [enabled, setEnabled] = useState(organization.enabled)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await onSave({ name: name.trim(), enabled })
    } catch {
      return
    }
  }

  return (
    <form className="flex max-w-xl flex-col gap-4" onSubmit={(event) => void submit(event)}>
      <FieldGroup>
        <Field><FieldLabel htmlFor={`${id}-name`}>{t("common.name")}</FieldLabel><Input id={`${id}-name`} value={name} required onChange={(event) => setName(event.target.value)} /></Field>
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-enabled`}>{t("users.fields.enabled")}</FieldLabel><Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} /></Field>
      </FieldGroup>
      <Button className="self-start" disabled={pending || !name.trim()}>{t(pending ? "common.actions.saving" : "common.actions.save")}</Button>
    </form>
  )
}
