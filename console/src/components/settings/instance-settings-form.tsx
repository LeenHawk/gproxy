import type { InstanceSettingsDto } from "@/generated/InstanceSettingsDto"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveInstanceSettings } from "@/api/control"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

type BooleanKey = "enable_downstream_log" | "enable_downstream_log_body" | "enable_upstream_log" | "enable_upstream_log_body"

export function InstanceSettingsForm({ settings }: { settings: InstanceSettingsDto }) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(settings)
  const [retention, setRetention] = useState(settings.retention_days?.toString() ?? "")
  const [size, setSize] = useState(settings.max_database_size_mb?.toString() ?? "")
  const mutation = useMutation({
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
    <form onSubmit={submit}>
      <Card>
        <CardHeader><CardTitle>{t("settings.storage.title")}</CardTitle><CardDescription>{t("settings.storage.description")}</CardDescription></CardHeader>
        <CardContent><FieldGroup>
          <div className="grid gap-4 sm:grid-cols-2">
            <Field><FieldLabel htmlFor="retention-days">{t("settings.storage.retention")}</FieldLabel><Input id="retention-days" type="number" min={1} step={1} value={retention} onChange={(event) => setRetention(event.target.value)} /><FieldDescription>{t("settings.storage.retentionHint")}</FieldDescription></Field>
            <Field><FieldLabel htmlFor="database-size">{t("settings.storage.size")}</FieldLabel><Input id="database-size" type="number" min={1} step={1} value={size} onChange={(event) => setSize(event.target.value)} /><FieldDescription>{t("settings.storage.sizeHint")}</FieldDescription></Field>
          </div>
          <div className="flex flex-col gap-3">
            {logRows.map((key) => <Field key={key} orientation="horizontal"><div><FieldLabel htmlFor={key}>{t(`settings.logs.${key}`)}</FieldLabel><FieldDescription>{t(`settings.logs.${key}Hint`)}</FieldDescription></div><Switch id={key} checked={draft[key]} onCheckedChange={(value) => toggle(key, value)} /></Field>)}
          </div>
          <Alert variant={draft.disable_log_redaction ? "destructive" : "default"}>
            <AlertTitle>{t("settings.redaction.title")}</AlertTitle>
            <AlertDescription className="flex flex-col gap-3">
              <p>{t(draft.disable_log_redaction ? "settings.redaction.disabledMeaning" : "settings.redaction.enabledMeaning")}</p>
              <Field orientation="horizontal"><FieldLabel htmlFor="disable-redaction">{t("settings.redaction.disable")}</FieldLabel><Switch id="disable-redaction" checked={draft.disable_log_redaction} onCheckedChange={(value) => setDraft((current) => ({ ...current, disable_log_redaction: value }))} /></Field>
            </AlertDescription>
          </Alert>
        </FieldGroup></CardContent>
        <CardFooter className="justify-end"><Button type="submit" disabled={mutation.isPending}>{t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}</Button></CardFooter>
      </Card>
    </form>
  )
}
