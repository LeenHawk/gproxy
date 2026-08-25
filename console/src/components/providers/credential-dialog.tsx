import type { FormEvent, ReactElement } from "react"
import type { CredentialDto } from "@/generated/CredentialDto"
import type { CredentialWriteRequest } from "@/generated/CredentialWriteRequest"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { parseJson } from "@/components/providers/json"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

type Props = {
  providerId: number
  credential?: CredentialDto
  trigger: ReactElement
  onSave: (value: CredentialWriteRequest, id?: number) => Promise<void>
}

export function CredentialDialog(props: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [open, setOpen] = useState(false)
  const [label, setLabel] = useState(props.credential?.label ?? "")
  const [secret, setSecret] = useState("")
  const [enabled, setEnabled] = useState(props.credential?.enabled ?? true)
  const [secretError, setSecretError] = useState("")
  const [submitError, setSubmitError] = useState("")
  const [saving, setSaving] = useState(false)

  const reset = () => {
    setLabel(props.credential?.label ?? "")
    setSecret("")
    setEnabled(props.credential?.enabled ?? true)
    setSecretError("")
    setSubmitError("")
  }

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const trimmedSecret = secret.trim()
    const parsed = trimmedSecret ? parseJson(trimmedSecret) : null
    const invalid = !props.credential && !trimmedSecret
      ? t("common.errors.required")
      : parsed && (!parsed.ok || parsed.value === null)
        ? t("common.errors.invalid")
        : ""
    setSecretError(invalid)
    setSubmitError("")
    if (invalid || (parsed && !parsed.ok)) return
    const value: CredentialWriteRequest = {
      provider_id: props.providerId,
      label: label.trim() || null,
      secret: parsed?.value ?? null,
      enabled,
    }
    setSaving(true)
    try {
      await props.onSave(value, props.credential?.id)
      toast.success(t(props.credential ? "providers.credentials.updated" : "providers.credentials.created"))
      setOpen(false)
    } catch {
      setSubmitError(t(props.credential ? "providers.credentials.updateError" : "providers.credentials.createError"))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(value) => { setOpen(value); if (value) reset() }}>
      <DialogTrigger asChild>{props.trigger}</DialogTrigger>
      <DialogContent className="sm:max-w-xl" showCloseButton={false}>
        <form className="flex flex-col gap-5" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>{t(props.credential ? "common.actions.edit" : "providers.credentials.add")}</DialogTitle>
            <DialogDescription>{t("providers.credentials.secretHint")}</DialogDescription>
          </DialogHeader>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${id}-label`}>{t("providers.credentials.label")}</FieldLabel>
              <Input id={`${id}-label`} value={label} onChange={(event) => setLabel(event.target.value)} />
            </Field>
            <Field data-invalid={Boolean(secretError) || undefined}>
              <FieldLabel htmlFor={`${id}-secret`}>{t("providers.credentials.secret")}</FieldLabel>
              <Textarea
                id={`${id}-secret`}
                className="machine-text min-h-40"
                value={secret}
                onChange={(event) => setSecret(event.target.value)}
                aria-invalid={Boolean(secretError) || undefined}
              />
              <FieldDescription>
                {t(props.credential ? "providers.credentials.keepSecret" : "providers.credentials.secretHint")}
              </FieldDescription>
              {secretError ? <FieldError>{secretError}</FieldError> : null}
            </Field>
            <Field orientation="horizontal">
              <FieldLabel htmlFor={`${id}-enabled`}>{t("providers.credentials.enabled")}</FieldLabel>
              <Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} />
            </Field>
          </FieldGroup>
          {submitError ? <FieldError>{submitError}</FieldError> : null}
          <DialogFooter>
            <DialogClose asChild><Button type="button" variant="outline">{t("common.actions.cancel")}</Button></DialogClose>
            <Button type="submit" disabled={saving}>{t(saving ? "common.actions.saving" : props.credential ? "common.actions.save" : "common.actions.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
