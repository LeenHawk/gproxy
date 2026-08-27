import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { useTranslation } from "react-i18next"
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { inputValue, settingValue, updateSetting } from "./settings-values"

export function GenericSettingsFields({
  fields,
  values,
  onChange,
}: {
  fields: Array<ChannelFieldDto>
  values: Record<string, unknown>
  onChange: (values: Record<string, unknown>) => void
}) {
  const { t } = useTranslation()
  return fields.map((field) => {
    const id = `provider-setting-${field.key}`
    const value = settingValue(field, values)
    const text = {
      label: t(`providers.channelFields.${field.i18n_key}.label`),
      description: t(`providers.channelFields.${field.i18n_key}.description`),
    }
    if (field.control === "boolean") {
      return (
        <Field key={field.key} orientation="horizontal">
          <FieldContent>
            <FieldLabel htmlFor={id}>{text.label}</FieldLabel>
            <FieldDescription>{text.description}</FieldDescription>
          </FieldContent>
          <Switch
            id={id}
            checked={value === true}
            onCheckedChange={(next) => onChange(updateSetting(values, field, next))}
          />
        </Field>
      )
    }
    if (field.control === "select") {
      return (
        <Field key={field.key}>
          <FieldLabel htmlFor={id}>{text.label}</FieldLabel>
          <Select value={typeof value === "string" ? value : ""} onValueChange={(next) => onChange(updateSetting(values, field, next))}>
            <SelectTrigger id={id} className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent><SelectGroup>{field.options.map((option) => <SelectItem key={option} value={option}>{t(`providers.channelFieldOptions.${field.i18n_key}.${option}`)}</SelectItem>)}</SelectGroup></SelectContent>
          </Select>
          <FieldDescription>{text.description}</FieldDescription>
        </Field>
      )
    }
    return (
      <Field key={field.key}>
        <FieldLabel htmlFor={id}>{text.label}</FieldLabel>
        <Input
          id={id}
          type={field.control === "secret" ? "password" : field.control === "url" ? "url" : field.control === "integer" ? "number" : "text"}
          step={field.control === "integer" ? 1 : undefined}
          required={field.required}
          className={field.control === "secret" ? undefined : "font-mono"}
          value={inputValue(field, value)}
          onChange={(event) => onChange(updateSetting(values, field, event.target.value))}
        />
        <FieldDescription>{text.description}</FieldDescription>
      </Field>
    )
  })
}
