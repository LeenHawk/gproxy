import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveInstanceSettings } from "@/api/control"
import { Section } from "@/components/section"
import { Field, FieldContent, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"
import { RuntimeSettingsCard } from "@/components/settings/runtime-settings-card"
import { TrafficBlacklistSection } from "@/components/settings/traffic-blacklist-section"
import { INSTANCE_SETTINGS_FORM_ID, INSTANCE_SETTINGS_MUTATION_KEY } from "./instance-settings-state"

type BooleanKey = "enable_downstream_log" | "enable_downstream_log_body" | "enable_upstream_log" | "enable_upstream_log_body"

export function InstanceSettingsForm({ settings }: { settings: InstanceSettingsDto }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(settings)
  const [retention, setRetention] = useState(settings.retention_days?.toString() ?? "")
  const [size, setSize] = useState(settings.max_database_size_mb?.toString() ?? "")
  const mutation = useMutation({
    mutationKey: INSTANCE_SETTINGS_MUTATION_KEY,
    mutationFn: saveInstanceSettings,
    onSuccess: async (value) => {
      setDraft(value)
      await queryClient.invalidateQueries({ queryKey: ["instance-settings"] })
      toast.success(t("settings.saved"))
    },
    onError: () => toast.error(t("settings.saveError")),
  })
  const toggle = (key: BooleanKey, value: boolean) => setDraft((current) => ({ ...current, [key]: value }))
  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    mutation.mutate({
      ...draft,
      retention_days: retention.trim() ? Number(retention) : null,
      max_database_size_mb: size.trim() ? Number(size) : null,
    })
  }
  const logRows: Array<BooleanKey> = [
    "enable_downstream_log",
    "enable_downstream_log_body",
    "enable_upstream_log",
    "enable_upstream_log_body",
  ]
  return (
    <form id={INSTANCE_SETTINGS_FORM_ID} className="flex flex-col gap-8" onSubmit={submit}>
      <RuntimeSettingsCard draft={draft} setDraft={setDraft} />
      <TrafficBlacklistSection
        defaults={draft.traffic_blacklist_defaults}
        value={draft.traffic_blacklist}
        onChange={(value) => setDraft((current) => ({ ...current, traffic_blacklist: value }))}
      />
      <Section title={t("settings.storage.title")} description={t("settings.storage.description")}>
        <FieldGroup>
          <Field><FieldLabel htmlFor="retention-days">{t("settings.storage.retention")}</FieldLabel><Input id="retention-days" type="number" min={1} step={1} value={retention} onChange={(event) => setRetention(event.target.value)} /><FieldDescription>{t("settings.storage.retentionHint")}</FieldDescription></Field>
          <Field><FieldLabel htmlFor="database-size">{t("settings.storage.size")}</FieldLabel><Input id="database-size" type="number" min={1} step={1} value={size} onChange={(event) => setSize(event.target.value)} /><FieldDescription>{t("settings.storage.sizeHint")}</FieldDescription></Field>
          <div data-field-span="full" className="flex flex-col gap-3">
            {logRows.map((key) => <Field key={key} orientation="horizontal"><FieldContent><FieldLabel htmlFor={key}>{t(`settings.logs.${key}`)}</FieldLabel><FieldDescription>{t(`settings.logs.${key}Hint`)}</FieldDescription></FieldContent><Switch id={key} checked={draft[key]} onCheckedChange={(value) => toggle(key, value)} /></Field>)}
            {/* Redaction reads as one more capture switch until it is off, which is the state worth interrupting for. */}
            <Field orientation="horizontal" className={cn(draft.disable_log_redaction && "rounded-lg border border-destructive/40 bg-destructive/5 p-3")}>
              <FieldContent>
                <FieldLabel htmlFor="disable-redaction" className={cn(draft.disable_log_redaction && "text-destructive")}>{t("settings.redaction.disable")}</FieldLabel>
                <FieldDescription>{t(draft.disable_log_redaction ? "settings.redaction.disabledMeaning" : "settings.redaction.enabledMeaning")}</FieldDescription>
              </FieldContent>
              <Switch id="disable-redaction" checked={draft.disable_log_redaction} onCheckedChange={(value) => setDraft((current) => ({ ...current, disable_log_redaction: value }))} />
            </Field>
          </div>
        </FieldGroup>
      </Section>
    </form>
  )
}
