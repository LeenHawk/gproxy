import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import { useId } from "react"
import { useTranslation } from "react-i18next"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { DEFAULT_FINGERPRINT, CUSTOM_FINGERPRINT } from "@/components/providers/fingerprint"

type Props = {
  text: string
  preset: string
  presets: Array<TlsPresetDto>
  presetsLoading: boolean
  presetsError: boolean
  validationError: string
  serverError?: string | null
  onTextChange: (value: string) => void
  onPresetChange: (value: string) => void
}

export function FingerprintField(props: Props) {
  const { t } = useTranslation()
  const id = useId()
  const error = props.validationError || props.serverError || ""

  return (
    <FieldSet>
      <FieldLegend>{t("providers.fingerprint.title")}</FieldLegend>
      <FieldDescription>{t("providers.fingerprint.description")}</FieldDescription>
      <FieldGroup>
        <Field data-invalid={props.presetsError || undefined}>
          <FieldLabel htmlFor={`${id}-preset`}>{t("providers.fingerprint.preset")}</FieldLabel>
          <Select value={props.preset} onValueChange={props.onPresetChange} disabled={props.presetsLoading}>
            <SelectTrigger id={`${id}-preset`} className="w-full" aria-invalid={props.presetsError || undefined}>
              <SelectValue placeholder={props.presetsLoading ? t("common.loading") : t("providers.fingerprint.defaultPreset")} />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                <SelectItem value={DEFAULT_FINGERPRINT}>{t("providers.fingerprint.defaultPreset")}</SelectItem>
                {props.presets.map((preset) => <SelectItem key={preset.id} value={preset.id}>{preset.label}</SelectItem>)}
                <SelectItem value={CUSTOM_FINGERPRINT}>{t("providers.fingerprint.custom")}</SelectItem>
              </SelectGroup>
            </SelectContent>
          </Select>
          {props.presetsError ? <FieldError>{t("providers.fingerprint.presetError")}</FieldError> : null}
        </Field>
        <Field data-field-span="full" data-invalid={Boolean(error) || undefined}>
          <FieldLabel htmlFor={`${id}-json`}>{t("providers.fingerprint.custom")}</FieldLabel>
          <Textarea
            id={`${id}-json`}
            className="machine-text"
            rows={4}
            value={props.text}
            onChange={(event) => props.onTextChange(event.target.value)}
            aria-invalid={Boolean(error) || undefined}
          />
          <FieldDescription>{t("providers.fingerprint.hint")}</FieldDescription>
          {error ? <FieldError>{error}</FieldError> : null}
        </Field>
      </FieldGroup>
    </FieldSet>
  )
}
