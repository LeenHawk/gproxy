import type { TrafficBlacklistDto } from "@/generated/TrafficBlacklistDto"
import { useTranslation } from "react-i18next"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel, FieldLegend, FieldSet } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"

type BlacklistKey = keyof TrafficBlacklistDto

const EMPTY: TrafficBlacklistDto = {
  request_headers: [],
  response_headers: [],
  request_query: [],
}

function lines(values: Array<string>) {
  return values.join("\n")
}

function parseLines(value: string) {
  return value.split("\n").map((entry) => entry.trim()).filter(Boolean)
}

export function TrafficBlacklistSection({
  defaults,
  value,
  onChange,
}: {
  defaults: TrafficBlacklistDto
  value: TrafficBlacklistDto
  onChange: (value: TrafficBlacklistDto) => void
}) {
  const { t } = useTranslation()
  const fields: Array<BlacklistKey> = ["request_headers", "response_headers", "request_query"]
  const restore = () => onChange({ ...EMPTY })
  return (
    <Section
      title={t("settings.trafficBlacklist.title")}
      description={t("settings.trafficBlacklist.description")}
      actions={<Button type="button" variant="outline" size="sm" onClick={restore}>{t("settings.trafficBlacklist.restore")}</Button>}
    >
      <FieldGroup>
        {fields.map((key) => (
          <FieldSet key={key} className={key === "request_query" ? "sm:col-span-2" : undefined}>
            <FieldLegend variant="label">{t(`settings.trafficBlacklist.${key}.label`)}</FieldLegend>
            <FieldDescription>{t(`settings.trafficBlacklist.${key}.description`)}</FieldDescription>
            <FieldGroup>
              <Field data-disabled>
                <FieldLabel htmlFor={`traffic-blacklist-default-${key}`}>{t("settings.trafficBlacklist.builtIn")}</FieldLabel>
                <Textarea id={`traffic-blacklist-default-${key}`} className="machine-text" value={lines(defaults[key])} disabled readOnly />
              </Field>
              <Field>
                <FieldLabel htmlFor={`traffic-blacklist-extra-${key}`}>{t("settings.trafficBlacklist.additional")}</FieldLabel>
                <Textarea
                  id={`traffic-blacklist-extra-${key}`}
                  className="machine-text"
                  value={lines(value[key])}
                  onChange={(event) => onChange({ ...value, [key]: parseLines(event.target.value) })}
                />
              </Field>
            </FieldGroup>
          </FieldSet>
        ))}
      </FieldGroup>
    </Section>
  )
}
