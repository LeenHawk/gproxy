import { useId } from "react"
import { useTranslation } from "react-i18next"
import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { TeamDto } from "@/generated/TeamDto"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

export type SubjectSelectProps = {
  organizations: Array<OrganizationDto>
  teams: Array<TeamDto>
  users: Array<UserDto>
  keys: Array<UserKeyDto>
  kind: string
  subjectId: string
  onChange: (kind: string, subjectId: string) => void
}

export function SubjectSelect(props: SubjectSelectProps) {
  const { t } = useTranslation()
  const id = useId()
  const labels: Record<string, string> = {
    organization: t("users.fields.organization"),
    team: t("users.fields.team"),
    user: t("access.subjectKinds.user"),
    user_key: t("access.subjectKinds.userKey"),
  }
  const options = props.kind === "organization"
    ? props.organizations
    : props.kind === "team"
      ? props.teams
      : props.kind === "user"
        ? props.users
        : props.keys

  return (
    <FieldGroup className="grid sm:grid-cols-2">
      <Field>
        <FieldLabel id={`${id}-kind-label`}>{t("access.subject")}</FieldLabel>
        <ToggleGroup type="single" variant="outline" value={props.kind} aria-labelledby={`${id}-kind-label`} className="flex-wrap justify-start" onValueChange={(kind) => { if (kind) props.onChange(kind, "") }}>
          {Object.entries(labels).map(([kind, label]) => <ToggleGroupItem key={kind} value={kind}>{label}</ToggleGroupItem>)}
        </ToggleGroup>
      </Field>
      <Field>
        <FieldLabel htmlFor={`${id}-subject`}>{labels[props.kind]}</FieldLabel>
        <Select value={props.subjectId} onValueChange={(subjectId) => props.onChange(props.kind, subjectId)}>
          <SelectTrigger id={`${id}-subject`}><SelectValue placeholder={t("common.required")} /></SelectTrigger>
          <SelectContent><SelectGroup>{options.map((option) => (
            <SelectItem key={option.id} value={String(option.id)}>
              {"label" in option ? option.label ?? option.prefix ?? String(option.id) : option.name}
            </SelectItem>
          ))}</SelectGroup></SelectContent>
        </Select>
      </Field>
    </FieldGroup>
  )
}
