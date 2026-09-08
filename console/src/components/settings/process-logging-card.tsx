import type { Dispatch, SetStateAction } from "react"
import { useTranslation } from "react-i18next"
import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import type { LogFormatDto } from "@/generated/LogFormatDto"
import type { LogLevelDto } from "@/generated/LogLevelDto"
import { Section } from "@/components/section"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { RuntimeSettingStatus } from "./runtime-setting-status"

const levels: LogLevelDto[] = ["off", "error", "warn", "info", "debug", "trace"]
const formats: LogFormatDto[] = ["text", "json"]

export function ProcessLoggingCard({ draft, setDraft }: {
  draft: InstanceSettingsDto
  setDraft: Dispatch<SetStateAction<InstanceSettingsDto>>
}) {
  const { t } = useTranslation()
  return (
    <Section title={t("settings.processLogs.title")} description={t("settings.processLogs.description")}>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="process-log-level">{t("settings.processLogs.level")}</FieldLabel>
          <Select value={draft.log_level} onValueChange={(value) => setDraft((current) => ({ ...current, log_level: value as LogLevelDto }))}>
            <SelectTrigger id="process-log-level"><SelectValue /></SelectTrigger>
            <SelectContent>{levels.map((level) => <SelectItem key={level} value={level}>{t(`settings.processLogs.levels.${level}`)}</SelectItem>)}</SelectContent>
          </Select>
          <FieldDescription>{t("settings.processLogs.levelHint")}</FieldDescription>
          <RuntimeSettingStatus draft={draft} field="log_level" />
        </Field>
        <Field>
          <FieldLabel htmlFor="process-log-format">{t("settings.processLogs.format")}</FieldLabel>
          <Select value={draft.log_format} onValueChange={(value) => setDraft((current) => ({ ...current, log_format: value as LogFormatDto }))}>
            <SelectTrigger id="process-log-format"><SelectValue /></SelectTrigger>
            <SelectContent>{formats.map((format) => <SelectItem key={format} value={format}>{t(`settings.processLogs.formats.${format}`)}</SelectItem>)}</SelectContent>
          </Select>
          <FieldDescription>{t("settings.processLogs.formatHint")}</FieldDescription>
          <RuntimeSettingStatus draft={draft} field="log_format" />
        </Field>
      </FieldGroup>
    </Section>
  )
}
