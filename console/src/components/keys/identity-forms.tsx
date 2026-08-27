import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"

type IdentityFormsProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  pending: boolean
  onOrganization: (name: string) => Promise<void>
  onTeam: (organizationId: number, name: string) => Promise<void>
  onUser: (organizationId: number | null, teamId: number | null, name: string) => Promise<void>
}

export function IdentityForms(props: IdentityFormsProps) {
  const { t } = useTranslation()
  const id = useId()
  const [name, setName] = useState("")
  const [teamOrganizationId, setTeamOrganizationId] = useState("")
  const [userOrganizationId, setUserOrganizationId] = useState("")
  const [userTeamId, setUserTeamId] = useState("")

  const submit = (action: () => Promise<void>) => async (event: FormEvent) => {
    event.preventDefault()
    try {
      await action()
      setName("")
    } catch {
      return
    }
  }

  const nameField = (suffix: string) => (
    <Field>
      <FieldLabel htmlFor={`${id}-${suffix}-name`}>{t("common.name")}</FieldLabel>
      <Input id={`${id}-${suffix}-name`} value={name} required onChange={(event) => setName(event.target.value)} />
    </Field>
  )
  const organizationField = (suffix: string, value: string, onChange: (value: string) => void, optional = false) => (
    <Field>
      <FieldLabel htmlFor={`${id}-${suffix}-organization`}>{t("users.fields.organization")}</FieldLabel>
      <Select value={value || (optional ? "none" : "")} onValueChange={(next) => onChange(next === "none" ? "" : next)}>
        <SelectTrigger id={`${id}-${suffix}-organization`}><SelectValue placeholder={t(optional ? "common.optional" : "common.required")} /></SelectTrigger>
        <SelectContent><SelectGroup>
          {optional ? <SelectItem value="none">{t("common.none")}</SelectItem> : null}
          {props.organizations.map((organization) => <SelectItem key={organization.id} value={String(organization.id)}>{organization.name}</SelectItem>)}
        </SelectGroup></SelectContent>
      </Select>
    </Field>
  )

  return (
    <Tabs defaultValue="user">
      <TabsList className="max-w-full overflow-x-auto overflow-y-hidden">
        <TabsTrigger value="user">{t("access.subjectKinds.user")}</TabsTrigger>
        <TabsTrigger value="team">{t("users.fields.team")}</TabsTrigger>
        <TabsTrigger value="organization">{t("users.fields.organization")}</TabsTrigger>
      </TabsList>
      <TabsContent value="organization" className="pt-5">
        <form className="flex max-w-md flex-col gap-4" onSubmit={submit(() => props.onOrganization(name.trim()))}>
          <FieldGroup>{nameField("organization")}</FieldGroup>
          <Button className="self-start" disabled={props.pending || !name.trim()}>{t(props.pending ? "common.actions.saving" : "common.actions.create")}</Button>
        </form>
      </TabsContent>
      <TabsContent value="team" className="pt-5">
        <form className="flex max-w-md flex-col gap-4" onSubmit={submit(() => props.onTeam(Number(teamOrganizationId), name.trim()))}>
          <FieldGroup>{organizationField("team", teamOrganizationId, setTeamOrganizationId)}{nameField("team")}</FieldGroup>
          <Button className="self-start" disabled={props.pending || !teamOrganizationId || !name.trim()}>{t(props.pending ? "common.actions.saving" : "common.actions.create")}</Button>
        </form>
      </TabsContent>
      <TabsContent value="user" className="pt-5">
        <form className="flex max-w-md flex-col gap-4" onSubmit={submit(() => props.onUser(userOrganizationId ? Number(userOrganizationId) : null, userTeamId ? Number(userTeamId) : null, name.trim()))}>
          <FieldGroup>
            {organizationField("user", userOrganizationId, (value) => { setUserOrganizationId(value); setUserTeamId("") }, true)}
            <Field>
              <FieldLabel htmlFor={`${id}-user-team`}>{t("users.fields.team")}</FieldLabel>
              <Select value={userTeamId || "none"} onValueChange={(value) => setUserTeamId(value === "none" ? "" : value)} disabled={!userOrganizationId}>
                <SelectTrigger id={`${id}-user-team`}><SelectValue placeholder={t("common.optional")} /></SelectTrigger>
                <SelectContent><SelectGroup>
                  <SelectItem value="none">{t("common.none")}</SelectItem>
                  {props.teams.filter((team) => team.organization_id === Number(userOrganizationId)).map((team) => <SelectItem key={team.id} value={String(team.id)}>{team.name}</SelectItem>)}
                </SelectGroup></SelectContent>
              </Select>
            </Field>
            {nameField("user")}
          </FieldGroup>
          <Button className="self-start" disabled={props.pending || !name.trim()}>{t(props.pending ? "common.actions.saving" : "common.actions.create")}</Button>
        </form>
      </TabsContent>
    </Tabs>
  )
}
