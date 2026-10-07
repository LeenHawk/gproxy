import { useId } from "react"
import { useTranslation } from "react-i18next"
import type { ConditionDraft } from "./conditions"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"

export function ConditionFields({ kind, value, onChange }: { kind: "body" | "header"; value: ConditionDraft; onChange: (value: ConditionDraft) => void }) {
  const { t } = useTranslation(), id = useId()
  const label = t(kind === "body" ? "rules.conditions.body" : "rules.conditions.header")
  return <FieldGroup>
    <Field>
      <FieldLabel htmlFor={`${id}-key`}>{label}</FieldLabel>
      <Input id={`${id}-key`} value={value.key} placeholder={kind === "body" ? "reasoning.effort" : "x-client-mode"} onChange={event => onChange({ ...value, key: event.target.value })} aria-describedby={`${id}-help`} />
      <FieldDescription id={`${id}-help`}>{t(kind === "body" ? "rules.conditions.bodyHelp" : "rules.conditions.headerHelp")}</FieldDescription>
    </Field>
    {value.key.trim() ? <>
      <Field>
        <FieldLabel htmlFor={`${id}-op`}>{t("rules.conditions.operator")}</FieldLabel>
        <Select value={value.op} onValueChange={op => onChange({ ...value, op: op as ConditionDraft["op"] })}>
          <SelectTrigger id={`${id}-op`} aria-label={`${label} ${t("rules.conditions.operator")}`}><SelectValue /></SelectTrigger>
          <SelectContent><SelectGroup>{(["eq", "ne", "exists", "not_exists"] as const).map(op => <SelectItem key={op} value={op}>{t(`rules.conditions.${op}`)}</SelectItem>)}</SelectGroup></SelectContent>
        </Select>
      </Field>
      {value.op === "eq" || value.op === "ne" ? <Field>
        <FieldLabel htmlFor={`${id}-value`}>{t(kind === "body" ? "rules.jsonValue" : "rules.conditions.value")}</FieldLabel>
        <Input id={`${id}-value`} aria-label={`${label} ${t("rules.conditions.value")}`} value={value.value} placeholder={kind === "body" ? '"high"' : "fast"} onChange={event => onChange({ ...value, value: event.target.value })} />
      </Field> : null}
    </> : null}
  </FieldGroup>
}
