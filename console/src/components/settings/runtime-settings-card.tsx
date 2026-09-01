import type { Dispatch, SetStateAction } from "react"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { useTranslation } from "react-i18next"
import { Section } from "@/components/section"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { ConnectivityTest } from "@/components/connectivity-test"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { proxyProbe } from "@/lib/connectivity-probe"

type Props = {
  draft: InstanceSettingsDto
  setDraft: Dispatch<SetStateAction<InstanceSettingsDto>>
}

type ToggleKey = "enable_usage" | "enable_tokenizer_download" | "inherit_system_proxy"

export function RuntimeSettingsCard({ draft, setDraft }: Props) {
  const { t } = useTranslation()
  const set = <K extends keyof InstanceSettingsDto>(key: K, value: InstanceSettingsDto[K]) => {
    setDraft((current) => ({ ...current, [key]: value }))
  }
  const toggles: Array<ToggleKey> = [
    "enable_usage",
    "inherit_system_proxy",
  ]

  return (
    <Section title={t("settings.runtime.title")} description={t("settings.runtime.description")}>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="instance-name">{t("settings.runtime.instanceName")}</FieldLabel>
          <Input id="instance-name" required value={draft.instance_name} onChange={(event) => set("instance_name", event.target.value)} />
          <FieldDescription>{t("settings.runtime.instanceNameHint")}</FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor="upload-limit">{t("settings.runtime.uploadLimit")}</FieldLabel>
          <Input id="upload-limit" type="number" min={0} step={1} value={draft.file_upload_max_in_flight} onChange={(event) => set("file_upload_max_in_flight", Number(event.target.value || 0))} />
          <FieldDescription>{t("settings.runtime.uploadLimitHint")}</FieldDescription>
        </Field>
        {/* The probe belongs on the field it probes, not in a box of its own below it. */}
        <Field data-field-span="full">
          <FieldLabel htmlFor="global-proxy">{t("settings.runtime.proxy")}</FieldLabel>
          <InputGroup>
            <InputGroupInput id="global-proxy" type="url" className="font-mono" value={draft.proxy ?? ""} onChange={(event) => set("proxy", event.target.value.trim() || null)} />
            <InputGroupAddon align="inline-end">
              <ConnectivityTest showLabel request={proxyProbe(draft.proxy ?? "", { provider_id: null, credential_id: null })} label={t("settings.runtime.connectivity")} />
            </InputGroupAddon>
          </InputGroup>
          <FieldDescription>{t("settings.runtime.proxyHint")}</FieldDescription>
        </Field>
        <div data-field-span="full" className="flex flex-col gap-3">
          {toggles.map((key) => (
            <Field key={key} orientation="horizontal">
              <FieldContent><FieldLabel htmlFor={key}>{t(`settings.runtime.${key}`)}</FieldLabel><FieldDescription>{t(`settings.runtime.${key}Hint`)}</FieldDescription></FieldContent>
              <Switch id={key} checked={draft[key]} onCheckedChange={(value) => set(key, value)} />
            </Field>
          ))}
        </div>
      </FieldGroup>
    </Section>
  )
}
