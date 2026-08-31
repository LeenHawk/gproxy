import type { TrafficPolicyDto } from "@/generated/TrafficPolicyDto"
import { useTranslation } from "react-i18next"
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

type PolicyKey = keyof TrafficPolicyDto

function lines(values: Array<string>) {
  return values.join("\n")
}

function parseLines(value: string) {
  return value.split("\n").map((entry) => entry.trim()).filter(Boolean)
}

function copy(policy: TrafficPolicyDto): TrafficPolicyDto {
  return {
    request_headers: [...policy.request_headers],
    response_headers: [...policy.response_headers],
    request_query: [...policy.request_query],
  }
}

export function TrafficPolicyFields({
  id,
  defaults,
  value,
  onChange,
}: {
  id: string
  defaults?: TrafficPolicyDto
  value: TrafficPolicyDto | null
  onChange: (value: TrafficPolicyDto | null) => void
}) {
  const { t } = useTranslation()
  if (!defaults) return null
  const custom = value !== null
  const effective = value ?? defaults
  const update = (key: PolicyKey, text: string) => onChange({ ...copy(effective), [key]: parseLines(text) })
  const fields: Array<{ key: PolicyKey; rows: number }> = [
    { key: "request_headers", rows: 4 },
    { key: "response_headers", rows: 4 },
    { key: "request_query", rows: 3 },
  ]
  return (
    <FieldSet className="sm:col-span-2">
      <FieldLegend>{t("providers.trafficPolicy.title")}</FieldLegend>
      <FieldDescription>{t("providers.trafficPolicy.description")}</FieldDescription>
      <Button type="button" variant="outline" size="sm" className="self-start" disabled={!custom} onClick={() => onChange(null)}>
        {t("providers.trafficPolicy.restore")}
      </Button>
      <FieldGroup>
        <Field orientation="horizontal" data-field-span="full">
          <FieldLabel htmlFor={`${id}-traffic-policy-custom`}>{t("providers.trafficPolicy.custom")}</FieldLabel>
          <Switch
            id={`${id}-traffic-policy-custom`}
            checked={custom}
            onCheckedChange={(checked) => onChange(checked ? copy(defaults) : null)}
          />
        </Field>
        {fields.map((field) => (
          <Field key={field.key} data-disabled={!custom || undefined} data-field-span={field.key === "request_query" ? "full" : undefined}>
            <FieldLabel htmlFor={`${id}-traffic-policy-${field.key}`}>{t(`providers.trafficPolicy.${field.key}.label`)}</FieldLabel>
            <Textarea
              id={`${id}-traffic-policy-${field.key}`}
              className="machine-text min-h-24"
              rows={field.rows}
              value={lines(effective[field.key])}
              disabled={!custom}
              onChange={(event) => update(field.key, event.target.value)}
            />
            <FieldDescription>{t(`providers.trafficPolicy.${field.key}.description`)}</FieldDescription>
          </Field>
        ))}
      </FieldGroup>
      <FieldDescription>{t("providers.trafficPolicy.blacklist")}</FieldDescription>
    </FieldSet>
  )
}
