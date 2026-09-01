import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export type IdentityKind = "user" | "team" | "organization"

type Props = {
  kind: IdentityKind
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  pending: boolean
  onOrganization: (name: string) => Promise<void>
  onTeam: (organizationId: number, name: string) => Promise<void>
  onUser: (organizationId: number | null, teamId: number | null, name: string, password: string) => Promise<void>
}

export function IdentityForm(props: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState("")
  const [organizationId, setOrganizationId] = useState("")
  const [teamId, setTeamId] = useState("")
  const [password, setPassword] = useState("")

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      if (props.kind === "organization") await props.onOrganization(name.trim())
      else if (props.kind === "team") await props.onTeam(Number(organizationId), name.trim())
      else await props.onUser(organizationId ? Number(organizationId) : null, teamId ? Number(teamId) : null, name.trim(), password)
      setName("")
      setPassword("")
    } catch {
      return
    }
  }

  const organizationRequired = props.kind === "team"
  return (
    <form className="flex flex-col gap-4" onSubmit={(event) => void submit(event)}>
      <FieldGroup>
        {props.kind !== "organization" ? <Field>
          <FieldLabel htmlFor={`${id}-organization`}>{t("users.fields.organization")}</FieldLabel>
          <Select value={organizationId || (organizationRequired ? "" : "none")} onValueChange={(value) => { setOrganizationId(value === "none" ? "" : value); setTeamId("") }}>
            <SelectTrigger id={`${id}-organization`}><SelectValue placeholder={t(organizationRequired ? "common.required" : "common.optional")} /></SelectTrigger>
            <SelectContent><SelectGroup>
              {organizationRequired ? null : <SelectItem value="none">{t("common.none")}</SelectItem>}
              {props.organizations.map((organization) => <SelectItem key={organization.id} value={String(organization.id)}>{organization.name}</SelectItem>)}
            </SelectGroup></SelectContent>
          </Select>
        </Field> : null}
        {props.kind === "user" ? <Field>
          <FieldLabel htmlFor={`${id}-team`}>{t("users.fields.team")}</FieldLabel>
          <Select value={teamId || "none"} onValueChange={(value) => setTeamId(value === "none" ? "" : value)} disabled={!organizationId}>
            <SelectTrigger id={`${id}-team`}><SelectValue placeholder={t("common.optional")} /></SelectTrigger>
            <SelectContent><SelectGroup>
              <SelectItem value="none">{t("common.none")}</SelectItem>
              {props.teams.filter((team) => team.organization_id === Number(organizationId)).map((team) => <SelectItem key={team.id} value={String(team.id)}>{team.name}</SelectItem>)}
            </SelectGroup></SelectContent>
          </Select>
        </Field> : null}
        <Field>
          <FieldLabel htmlFor={`${id}-name`}>{t("common.name")}</FieldLabel>
          <Input id={`${id}-name`} value={name} required onChange={(event) => setName(event.target.value)} />
        </Field>
        {props.kind === "user" ? <Field>
          <FieldLabel htmlFor={`${id}-password`}>{t("portal.login.password")}</FieldLabel>
          <Input id={`${id}-password`} type="password" autoComplete="new-password" value={password} required onChange={(event) => setPassword(event.target.value)} />
        </Field> : null}
      </FieldGroup>
      <Button className="self-start" disabled={props.pending || !name.trim() || (organizationRequired && !organizationId) || (props.kind === "user" && !password)}>{t(props.pending ? "common.actions.saving" : "common.actions.create")}</Button>
    </form>
  )
}
