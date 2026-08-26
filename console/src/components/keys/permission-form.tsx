import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { PermissionWriteRequest } from "@/generated/PermissionWriteRequest"
import type { ProviderDto } from "@/generated/ProviderDto"
import { SubjectSelect, type SubjectSelectProps } from "@/components/keys/subject-select"
import { Button } from "@/components/ui/button"
import { SearchableSelect } from "@/components/searchable-select"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"

type PermissionFormProps = Pick<SubjectSelectProps, "organizations" | "teams" | "users" | "keys"> & {
  providers: Array<ProviderDto>
  groups: Array<string>
  pending: boolean
  onSubmit: (value: PermissionWriteRequest) => Promise<void>
}

export function PermissionForm(props: PermissionFormProps) {
  const { t } = useTranslation()
  const id = useId()
  const [kind, setKind] = useState("user_key")
  const [subjectId, setSubjectId] = useState("")
  const [providerId, setProviderId] = useState("all")
  const [group, setGroup] = useState("all")
  const [effect, setEffect] = useState("allow")

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await props.onSubmit({
        subject_kind: kind,
        subject_id: Number(subjectId),
        provider_id: providerId === "all" ? null : Number(providerId),
        operation_group: group === "all" ? null : group,
        allowed: effect === "allow",
      })
    } catch {
      return
    }
  }

  return (
    <form className="flex flex-col gap-5" onSubmit={(event) => void submit(event)}>
      <SubjectSelect {...props} kind={kind} subjectId={subjectId} onChange={(nextKind, nextId) => { setKind(nextKind); setSubjectId(nextId) }} />
      <FieldGroup className="grid sm:grid-cols-3">
        <Field>
          <FieldLabel htmlFor={`${id}-provider`}>{t("access.permissions.provider")}</FieldLabel>
          <SearchableSelect id={`${id}-provider`} value={providerId} options={[{ value: "all", label: t("access.permissions.allProviders") }, ...props.providers.map((provider) => ({ value: String(provider.id), label: provider.name }))]} placeholder={t("common.none")} searchPlaceholder={t("common.search")} emptyLabel={t("common.none")} ariaLabel={t("access.permissions.provider")} onChange={setProviderId} />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-group`}>{t("access.permissions.operationGroup")}</FieldLabel>
          <Select value={group} onValueChange={setGroup}>
            <SelectTrigger id={`${id}-group`}><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>
              <SelectItem value="all">{t("access.permissions.allOperations")}</SelectItem>
              {props.groups.map((value) => <SelectItem key={value} value={value}>{value}</SelectItem>)}
            </SelectGroup></SelectContent>
          </Select>
        </Field>
        <Field>
          <FieldLabel id={`${id}-effect-label`}>{t("access.permissions.effect")}</FieldLabel>
          <ToggleGroup type="single" variant="outline" value={effect} aria-labelledby={`${id}-effect-label`} onValueChange={(value) => { if (value) setEffect(value) }}>
            <ToggleGroupItem value="allow">{t("access.permissions.allow")}</ToggleGroupItem>
            <ToggleGroupItem value="deny">{t("access.permissions.deny")}</ToggleGroupItem>
          </ToggleGroup>
        </Field>
      </FieldGroup>
      <Button className="self-start" disabled={props.pending || !subjectId}>{t("access.permissions.add")}</Button>
    </form>
  )
}
