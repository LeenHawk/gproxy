import type { Dispatch, SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import { PlusIcon, Trash2Icon } from "lucide-react"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { Section } from "@/components/section"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { RuntimeSettingStatus } from "./runtime-setting-status"

export function NetworkSettingsCard({ draft, setDraft }: {
  draft: InstanceSettingsDto
  setDraft: Dispatch<SetStateAction<InstanceSettingsDto>>
}) {
  const { t } = useTranslation()
  return (
    <Section title={t("settings.network.title")} description={t("settings.network.description")}>
      <FieldGroup>
        {(["cors_origins", "trusted_proxies"] as const).map((field) => (
          <Field key={field} data-field-span="full">
            <FieldLabel>{t(`settings.network.${field}`)}</FieldLabel>
            <FieldDescription>{t(`settings.network.${field}Hint`)}</FieldDescription>
            {draft[field].map((value, index) => (
              <div key={index} className="flex gap-2">
                <Input
                  aria-label={t("settings.network.entry", { label: t(`settings.network.${field}`), number: index + 1 })}
                  type={field === "cors_origins" ? "url" : "text"}
                  value={value}
                  placeholder={field === "cors_origins" ? "https://example.com" : "192.0.2.1"}
                  onChange={(event) => setDraft((current) => ({ ...current, [field]: current[field].map((entry, position) => position === index ? event.target.value : entry) }))}
                />
                <Button type="button" variant="outline" size="icon" aria-label={t("settings.network.remove", { label: t(`settings.network.${field}`), number: index + 1 })} onClick={() => setDraft((current) => ({ ...current, [field]: current[field].filter((_, position) => position !== index) }))}><Trash2Icon /></Button>
              </div>
            ))}
            <Button type="button" variant="outline" className="self-start" onClick={() => setDraft((current) => ({ ...current, [field]: [...current[field], ""] }))}><PlusIcon data-icon="inline-start" />{t(`settings.network.add_${field}`)}</Button>
            <RuntimeSettingStatus draft={draft} field={field} />
          </Field>
        ))}
      </FieldGroup>
    </Section>
  )
}
