import type { ReactNode } from "react"
import type { ChannelDto } from "@/generated/ChannelDto"
import { ChevronDownIcon, Code2Icon, SlidersHorizontalIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { parseJsonObject, prettyJson } from "@/components/providers/json"
import { Button } from "@/components/ui/button"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"
import { EndpointFields } from "./endpoint-fields"
import { GenericSettingsFields } from "./generic-settings-fields"
import { endpointRows, objectValue, updateEndpoints } from "./settings-values"

export function ProviderSettingsFields({
  channel,
  text,
  error,
  advancedChildren,
  onChange,
}: {
  channel?: ChannelDto
  text: string
  error: string
  advancedChildren?: ReactNode
  onChange: (text: string) => void
}) {
  const { t } = useTranslation()
  const parsed = parseJsonObject<Record<string, unknown>>(text)
  const values = parsed.ok ? objectValue(parsed.value) : {}
  const fields = channel?.provider_fields ?? []
  const basic = fields.filter((field) => !field.advanced)
  const advanced = fields.filter((field) => field.advanced)
  const kinds = channel?.endpoint_kinds ?? []
  const commit = (next: Record<string, unknown>) => onChange(prettyJson(next))
  return (
    <>
      <GenericSettingsFields fields={basic} values={values} onChange={commit} />
      <EndpointFields
        kinds={kinds}
        rows={endpointRows(values, kinds)}
        onChange={(rows) => commit(updateEndpoints(values, kinds, rows))}
      />
      <Collapsible data-field-span="full">
        <CollapsibleTrigger asChild>
          <Button type="button" variant="outline" className="group w-full justify-between">
            <span className="flex items-center gap-2"><SlidersHorizontalIcon />{t("providers.form.advanced")}</span>
            <ChevronDownIcon data-icon="inline-end" className="transition-transform group-data-[state=open]:rotate-180" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className="flex flex-col gap-4 pt-4">
          <GenericSettingsFields fields={advanced} values={values} onChange={commit} />
          {advancedChildren}
          <Collapsible>
            <CollapsibleTrigger asChild>
              <Button type="button" variant="ghost" size="sm" className="self-start">
                <Code2Icon data-icon="inline-start" />{t("providers.form.jsonEscape")}
              </Button>
            </CollapsibleTrigger>
            <CollapsibleContent>
              <Field data-invalid={Boolean(error) || undefined}>
                <FieldLabel htmlFor="provider-settings-json">{t("providers.fields.settings")}</FieldLabel>
                <Textarea
                  id="provider-settings-json"
                  className="machine-text min-h-32"
                  value={text}
                  aria-invalid={Boolean(error) || undefined}
                  onChange={(event) => onChange(event.target.value)}
                />
                <FieldDescription>{t("providers.form.settingsHint")}</FieldDescription>
                {error ? <FieldError>{error}</FieldError> : null}
              </Field>
            </CollapsibleContent>
          </Collapsible>
        </CollapsibleContent>
      </Collapsible>
    </>
  )
}
