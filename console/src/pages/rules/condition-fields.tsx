import { useId } from "react"
import { useTranslation } from "react-i18next"
import type { BodyConditionDraft, ConditionDraft } from "./conditions"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Textarea } from "@/components/ui/textarea"
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

export function BodyConditionFields({ value, onChange }: { value: BodyConditionDraft; onChange: (value: BodyConditionDraft) => void }) {
  const { t } = useTranslation(), id = useId()
  return <FieldGroup>
    <Field data-field-span="full">
      <FieldLabel>{t("rules.conditions.bodyMode")}</FieldLabel>
      <ToggleGroup type="single" variant="outline" aria-label={t("rules.conditions.bodyMode")} value={value.mode} onValueChange={mode => { if (mode === "fields" || mode === "jmespath") onChange({ ...value, mode }) }}>
        <ToggleGroupItem value="jmespath">JMESPath</ToggleGroupItem>
        <ToggleGroupItem value="fields">{t("rules.conditions.fieldCondition")}</ToggleGroupItem>
      </ToggleGroup>
    </Field>
    {value.mode === "fields" ? <ConditionFields kind="body" value={value.fields} onChange={fields => onChange({ ...value, fields })} /> : <Field data-field-span="full">
      <FieldLabel htmlFor={id}>{t("rules.conditions.expression")}</FieldLabel>
      <Textarea id={id} rows={4} spellCheck={false} value={value.expression} onChange={event => onChange({ ...value, expression: event.target.value })} aria-describedby={`${id}-help`} placeholder="contains(messages[-1].content[0].text, '[cache-keepalive]')" />
      <FieldDescription id={`${id}-help`}>{t("rules.conditions.expressionHelp")}</FieldDescription>
    </Field>}
  </FieldGroup>
}
