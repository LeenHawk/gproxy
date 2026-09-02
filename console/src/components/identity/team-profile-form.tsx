import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { TeamWriteRequest } from "@/generated/TeamWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

export function TeamProfileForm({ team, organizations, pending, onSave }: {
  team: TeamDto
  organizations: Array<OrganizationDto>
  pending: boolean
  onSave: (value: TeamWriteRequest) => Promise<void>
}) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState(team.name)
  const [organizationId, setOrganizationId] = useState(String(team.organization_id))
  const [enabled, setEnabled] = useState(team.enabled)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await onSave({ name: name.trim(), organization_id: Number(organizationId), enabled })
    } catch {
      return
    }
  }

  return (
    <form className="flex max-w-xl flex-col gap-4" onSubmit={(event) => void submit(event)}>
      <FieldGroup>
        <Field><FieldLabel htmlFor={`${id}-name`}>{t("common.name")}</FieldLabel><Input id={`${id}-name`} value={name} required onChange={(event) => setName(event.target.value)} /></Field>
        <Field>
          <FieldLabel htmlFor={`${id}-organization`}>{t("users.fields.organization")}</FieldLabel>
          <Select value={organizationId} onValueChange={setOrganizationId}>
            <SelectTrigger id={`${id}-organization`}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{organizations.map((organization) => <SelectItem key={organization.id} value={String(organization.id)}>{organization.name}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-enabled`}>{t("users.fields.enabled")}</FieldLabel><Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} /></Field>
      </FieldGroup>
      <Button className="self-start" disabled={pending || !name.trim() || !organizationId}>{t(pending ? "common.actions.saving" : "common.actions.save")}</Button>
    </form>
  )
}
