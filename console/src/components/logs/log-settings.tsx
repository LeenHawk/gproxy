import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { LogSettingsDto } from "@/generated/LogSettingsDto"
import type { LogSettingsUpdateDto } from "@/generated/LogSettingsUpdateDto"
import { saveLogSettings } from "@/api/observability"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldContent, FieldDescription, FieldLabel, FieldTitle } from "@/components/ui/field"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

const fields: Array<keyof LogSettingsUpdateDto> = [
  "enable_downstream_log",
  "enable_downstream_log_body",
  "enable_upstream_log",
  "enable_upstream_log_body",
  "disable_log_redaction",
]

function editable(value: LogSettingsDto): LogSettingsUpdateDto {
  return Object.fromEntries(fields.map((field) => [field, value[field]])) as unknown as LogSettingsUpdateDto
}

export function LogSettings({ value }: { value: LogSettingsDto }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [draft, setDraft] = useState(() => editable(value))
  const mutation = useMutation({
    mutationFn: saveLogSettings,
    onSuccess: (next) => {
      client.setQueryData(["log-settings"], next)
      setDraft(editable(next))
      toast.success(t("logs.settings.saved"))
    },
    onError: () => toast.error(t("logs.settings.saveError")),
  })
  const change = (field: keyof LogSettingsUpdateDto, checked: boolean) => setDraft((current) => ({ ...current, [field]: checked }))
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{t("logs.settings.title")}</CardTitle>
        <CardDescription>{t("logs.settings.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="grid gap-3 lg:grid-cols-2">
          {fields.map((field) => {
            const body = field.endsWith("_body")
            const danger = field === "disable_log_redaction"
            const disabled = body && !value.body_capture_allowed
            return (
              <Field key={field} orientation="horizontal" data-disabled={disabled || undefined} className={cn("rounded-lg border p-3", danger && draft[field] && "border-destructive/50 bg-destructive/5")}>
                <FieldContent>
                  <FieldLabel htmlFor={`log-setting-${field}`}><FieldTitle>{t(`logs.settings.${field}.label`)}</FieldTitle></FieldLabel>
                  <FieldDescription>{t(`logs.settings.${field}.description`)}</FieldDescription>
                </FieldContent>
                <Switch id={`log-setting-${field}`} checked={draft[field]} disabled={disabled} onCheckedChange={(checked) => change(field, checked)} />
              </Field>
            )
          })}
        </div>
        {!value.body_capture_allowed ? <p className="rounded-lg border border-state-warning/40 bg-state-warning/5 p-3 text-sm text-state-warning">{t("logs.settings.bodyBlocked")}</p> : <p className="text-sm text-muted-foreground">{t("logs.settings.retention", { days: value.retention_days ?? t("common.none"), size: value.max_database_size_mb ?? t("common.none") })}</p>}
        <div className="flex justify-end"><Button onClick={() => mutation.mutate(draft)} disabled={mutation.isPending}>{t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}</Button></div>
      </CardContent>
    </Card>
  )
}
