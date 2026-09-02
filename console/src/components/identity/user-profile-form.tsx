import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserWriteRequest } from "@/generated/UserWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"

export function UserProfileForm({ user, organizations, teams, pending, onSave }: {
  user: UserDto
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  pending: boolean
  onSave: (value: UserWriteRequest, password: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState(user.name)
  const [organizationId, setOrganizationId] = useState(user.organization_id == null ? "none" : String(user.organization_id))
  const [teamId, setTeamId] = useState(user.team_id == null ? "none" : String(user.team_id))
  const [password, setPassword] = useState("")
  const [enabled, setEnabled] = useState(user.enabled)
  const [isAdmin, setIsAdmin] = useState(user.is_admin)
  const scopedTeams = organizationId === "none" ? [] : teams.filter((team) => team.organization_id === Number(organizationId))

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await onSave({
        name: name.trim(),
        organization_id: organizationId === "none" ? null : Number(organizationId),
        team_id: teamId === "none" ? null : Number(teamId),
        enabled,
        is_admin: isAdmin,
        password: null,
      }, password)
      setPassword("")
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
          <Select value={organizationId} onValueChange={(value) => { setOrganizationId(value); setTeamId("none") }}>
            <SelectTrigger id={`${id}-organization`}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup><SelectItem value="none">{t("common.none")}</SelectItem>{organizations.map((organization) => <SelectItem key={organization.id} value={String(organization.id)}>{organization.name}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-team`}>{t("users.fields.team")}</FieldLabel>
          <Select value={teamId} onValueChange={setTeamId} disabled={organizationId === "none"}>
            <SelectTrigger id={`${id}-team`}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup><SelectItem value="none">{t("common.none")}</SelectItem>{scopedTeams.map((team) => <SelectItem key={team.id} value={String(team.id)}>{team.name}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-password`}>{t("auth.password")}</FieldLabel>
          <Input id={`${id}-password`} type="password" autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} />
          <FieldDescription>{t("users.fields.passwordKeep")}</FieldDescription>
        </Field>
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-admin`}>{t("users.roles.admin")}</FieldLabel><Switch id={`${id}-admin`} checked={isAdmin} onCheckedChange={setIsAdmin} /></Field>
        <Field orientation="horizontal"><FieldLabel htmlFor={`${id}-enabled`}>{t("users.fields.enabled")}</FieldLabel><Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} /></Field>
      </FieldGroup>
      <Button className="self-start" disabled={pending || !name.trim()}>{t(pending ? "common.actions.saving" : "common.actions.save")}</Button>
    </form>
  )
}
