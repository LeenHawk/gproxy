import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { humanizeSettingKey, inputValue, settingValue, updateSetting } from "./settings-values"

export function GenericSettingsFields({
  fields,
  values,
  onChange,
}: {
  fields: Array<ChannelFieldDto>
  values: Record<string, unknown>
  onChange: (values: Record<string, unknown>) => void
}) {
  return fields.map((field) => {
    const id = `provider-setting-${field.key}`
    const value = settingValue(field, values)
    const label = humanizeSettingKey(field.key)
    if (field.control === "boolean") {
      return (
        <Field key={field.key} orientation="horizontal">
          <FieldLabel htmlFor={id}>{label}</FieldLabel>
          <Switch
            id={id}
            checked={value === true}
            onCheckedChange={(next) => onChange(updateSetting(values, field, next))}
          />
        </Field>
      )
    }
    return (
      <Field key={field.key}>
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        <Input
          id={id}
          type={field.control === "secret" ? "password" : field.control === "url" ? "url" : field.control === "integer" ? "number" : "text"}
          step={field.control === "integer" ? 1 : undefined}
          required={field.required}
          className={field.control === "secret" ? undefined : "font-mono"}
          value={inputValue(field, value)}
          onChange={(event) => onChange(updateSetting(values, field, event.target.value))}
        />
      </Field>
    )
  })
}
