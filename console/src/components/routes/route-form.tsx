import { useId, useState, type ReactNode } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { saveRoute } from "@/api/control"
import type { RouteDto } from "@/generated/RouteDto"
import type { RouteWriteRequest } from "@/generated/RouteWriteRequest"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogClose, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { FormDialogContent } from "@/components/routes/form-dialog-content"

export function RouteEditor({ route, onChanged, onSaved, dialogAction }: {
  route: RouteDto | null
  onChanged: () => void
  onSaved?: () => void
  dialogAction?: ReactNode
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
    onSuccess: () => {
      toast.success(t(route ? "routes.form.updated" : "routes.form.created"))
      onChanged()
      onSaved?.()
    },
    onError: () => toast.error(t(route ? "routes.form.updateError" : "routes.form.createError")),
  })
  const fields = <FieldGroup>
    <Field><FieldLabel htmlFor={nameId}>{t("routes.fields.name")}</FieldLabel><Input id={nameId} value={name} required onChange={(event) => setName(event.target.value)} /></Field>
    <Field>
      <FieldLabel htmlFor={attemptsId}>{t("routes.fields.maxAttempts")}</FieldLabel>
      <Input id={attemptsId} type="number" min={1} step={1} value={maxAttempts} required onChange={(event) => setMaxAttempts(event.target.value)} />
      <FieldDescription>{t("routes.form.attemptsHint")}</FieldDescription>
    </Field>
    <Field orientation="horizontal"><FieldLabel htmlFor={enabledId}>{t("routes.fields.enabled")}</FieldLabel><Switch id={enabledId} checked={enabled} onCheckedChange={setEnabled} /></Field>
  </FieldGroup>
  const save = <Button type="submit" disabled={mutation.isPending || !name.trim()}>{t(mutation.isPending ? "common.actions.saving" : "common.actions.save")}</Button>
  const submit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    mutation.mutate({ name: name.trim(), max_attempts: Number(maxAttempts), enabled })
  }

  if (dialogAction) return <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit}><DialogBody>{fields}</DialogBody><DialogFooter>{dialogAction}{save}</DialogFooter></form>
  return <form className="flex max-w-xl flex-col gap-4" onSubmit={submit}>{fields}<div className="self-start">{save}</div></form>
}

export function RouteForm({ opener, onOpenChange, onChanged }: {
  opener: HTMLElement | null
  onOpenChange: (open: boolean) => void
  onChanged: () => void
}) {
  const { t } = useTranslation()
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <FormDialogContent opener={opener}>
        <DialogHeader><DialogTitle>{t("routes.form.createTitle")}</DialogTitle></DialogHeader>
        <RouteEditor route={null} onChanged={onChanged} onSaved={() => onOpenChange(false)} dialogAction={<DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>} />
      </FormDialogContent>
    </Dialog>
  )
}
