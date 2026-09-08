import type { Dispatch, SetStateAction } from "react"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { useTranslation } from "react-i18next"
import { Section } from "@/components/section"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { ConnectivityTest } from "@/components/connectivity-test"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import { RuntimeSettingStatus } from "./runtime-setting-status"
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
          <RuntimeSettingStatus draft={draft} field="file_upload_max_in_flight" />
        </Field>
        {draft.runtime_status?.native_controls !== false ? (
          <Field>
            <FieldLabel htmlFor="request-limit">{t("settings.runtime.requestLimit")}</FieldLabel>
            <Input id="request-limit" type="number" min={1} step={1} required value={draft.max_in_flight} onChange={(event) => set("max_in_flight", Number(event.target.value))} />
            <FieldDescription>{t("settings.runtime.requestLimitHint")}</FieldDescription>
            <RuntimeSettingStatus draft={draft} field="max_in_flight" />
          </Field>
        ) : null}
        <Field>
          <FieldLabel htmlFor="max-attempts">{t("settings.runtime.maxAttempts")}</FieldLabel>
          <Input id="max-attempts" type="number" min={1} max={4294967295} step={1} required value={draft.max_attempts} onChange={(event) => set("max_attempts", Number(event.target.value))} />
          <FieldDescription>{t("settings.runtime.maxAttemptsHint")}</FieldDescription>
          <RuntimeSettingStatus draft={draft} field="max_attempts" />
        </Field>
        <Field data-field-span="full">
          <FieldLabel htmlFor="global-proxy">{t("settings.runtime.proxy")}</FieldLabel>
          <InputGroup>
            <InputGroupInput id="global-proxy" type="url" className="font-mono" value={draft.proxy ?? ""} onChange={(event) => set("proxy", event.target.value.trim() || null)} />
            <InputGroupAddon align="inline-end">
              <ConnectivityTest showLabel request={proxyProbe(draft.proxy ?? "", { provider_id: null, credential_id: null })} label={t("settings.runtime.connectivity")} />
            </InputGroupAddon>
          </InputGroup>
          <FieldDescription>{t("settings.runtime.proxyHint")}</FieldDescription>
          <RuntimeSettingStatus draft={draft} field="proxy" />
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
