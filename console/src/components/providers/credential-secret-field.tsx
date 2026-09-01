import type { ChannelFieldDto } from "@/generated/ChannelFieldDto"
import { useTranslation } from "react-i18next"
import { isSingleKey, secretTemplate } from "@/components/providers/credential-secret"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"

export function CredentialSecretField({ fields, value, onChange, editing }: {
  fields: Array<ChannelFieldDto>
  value: string
  onChange: (text: string) => void
  editing: boolean
}) {
  const { t } = useTranslation()
  const single = isSingleKey(fields)
  const label = single
    ? t(`providers.channelFields.${fields[0].i18n_key}.label`)
    : t("providers.credentials.secretJson")
  return (
    <Field data-field-span="full">
      <FieldLabel htmlFor="credential-secret">{label}</FieldLabel>
      {single
        ? <Input id="credential-secret" className="machine-text" autoComplete="off" spellCheck={false} placeholder={editing ? "••••••••" : undefined} value={value} onChange={(event) => onChange(event.target.value)} />
        : <Textarea id="credential-secret" className="machine-text" rows={4} autoComplete="off" spellCheck={false} placeholder={secretTemplate(fields)} value={value} onChange={(event) => onChange(event.target.value)} />}
      {editing ? <FieldDescription>{t("providers.credentials.keepSecret")}</FieldDescription> : null}
    </Field>
  )
}
