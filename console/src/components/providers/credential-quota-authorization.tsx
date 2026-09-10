import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function CredentialQuotaAuthorization({ fields, values, onChange, editing, revealFailed }: {
  fields: Array<ChannelFieldDto>
  values: Record<string, string>
  onChange: (key: string, value: string) => void
  editing: boolean
  revealFailed: boolean
}) {
  const { t } = useTranslation()
  const id = useId()
  if (!fields.length) return null
  return <FieldSet data-field-span="full">
    <FieldLegend>{t("upstreamQuota.authorization.title")}</FieldLegend>
    <FieldDescription>{t("upstreamQuota.authorization.description")}</FieldDescription>
    {revealFailed ? <FieldDescription role="alert">{t("upstreamQuota.authorization.revealFailed")}</FieldDescription> : null}
    <FieldGroup>
      {fields.map((field) => {
        const label = t(`upstreamQuota.authorization.fields.${field.i18n_key}.label`, {
          defaultValue: t(`providers.channelFields.${field.i18n_key}.label`, { defaultValue: field.key }),
        })
        return <Field key={field.key}>
          <FieldLabel htmlFor={`${id}-${field.key}`}>{label}</FieldLabel>
          <div className="flex items-center gap-2">
            <Input id={`${id}-${field.key}`} type={field.control === "secret" ? "password" : field.control === "url" ? "url" : "text"}
              autoComplete="off" spellCheck={false} value={values[field.key] ?? ""}
              onChange={(event) => onChange(field.key, event.target.value)} />
            {editing ? <Button type="button" variant="outline" size="sm" aria-label={t("upstreamQuota.authorization.clearField", { field: label })}
              onClick={() => onChange(field.key, "")}>{t("upstreamQuota.authorization.clear")}</Button> : null}
          </div>
          <FieldDescription>{t(`upstreamQuota.authorization.fields.${field.i18n_key}.description`, { defaultValue: t("upstreamQuota.authorization.optional") })}</FieldDescription>
        </Field>
      })}
    </FieldGroup>
  </FieldSet>
}
