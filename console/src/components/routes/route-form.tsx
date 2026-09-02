import { useId, useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveRoute } from "@/api/control"
import type { IdResponse } from "@/generated/IdResponse"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteWriteRequest } from "@/generated/RouteWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

export function RouteEditor({ route, onChanged, onSaved }: {
  route: RouteDto | null
  onChanged: () => void
  onSaved?: (result: IdResponse | undefined) => void
}) {
  const { t } = useTranslation()
  const nameId = useId()
  const attemptsId = useId()
  const enabledId = useId()
  const [name, setName] = useState(route?.name ?? "")
  const [maxAttempts, setMaxAttempts] = useState(String(route?.max_attempts ?? 3))
  const [enabled, setEnabled] = useState(route?.enabled ?? true)
  const mutation = useMutation({
    mutationFn: (value: RouteWriteRequest) => saveRoute(value, route?.id),
    onSuccess: (result) => {
      toast.success(t(route ? "routes.form.updated" : "routes.form.created"))
      onChanged()
      onSaved?.(result)
    },
    onError: () => toast.error(t(route ? "routes.form.updateError" : "routes.form.createError")),
  })
  const submit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    mutation.mutate({ name: name.trim(), max_attempts: Number(maxAttempts), enabled })
  }

  return <form className="flex max-w-xl flex-col gap-4" onSubmit={submit}>
    <FieldGroup>
      <Field><FieldLabel htmlFor={nameId}>{t("routes.fields.name")}</FieldLabel><Input id={nameId} value={name} required onChange={(event) => setName(event.target.value)} /></Field>
      <Field>
        <FieldLabel htmlFor={attemptsId}>{t("routes.fields.maxAttempts")}</FieldLabel>
        <Input id={attemptsId} type="number" min={1} step={1} value={maxAttempts} required onChange={(event) => setMaxAttempts(event.target.value)} />
        <FieldDescription>{t("routes.form.attemptsHint")}</FieldDescription>
      </Field>
      <Field orientation="horizontal"><FieldLabel htmlFor={enabledId}>{t("routes.fields.enabled")}</FieldLabel><Switch id={enabledId} checked={enabled} onCheckedChange={setEnabled} /></Field>
    </FieldGroup>
    <Button className="self-start" type="submit" disabled={mutation.isPending || !name.trim()}>{t(mutation.isPending ? "common.actions.saving" : route ? "common.actions.save" : "common.actions.create")}</Button>
  </form>
}
