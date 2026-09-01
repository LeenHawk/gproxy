import { useId, useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import type { RateLimitWriteRequest } from "@/generated/RateLimitWriteRequest"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

type RateFormProps = {
  pending: boolean
  onSubmit: (value: RateLimitWriteRequest) => Promise<void>
  fixedSubject: { kind: string; id: number }
}

export function RateForm(props: RateFormProps) {
  const { t } = useTranslation()
  const id = useId()
  const [requests, setRequests] = useState("")
  const [seconds, setSeconds] = useState("")

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    try {
      await props.onSubmit({
        subject_kind: props.fixedSubject.kind,
        subject_id: props.fixedSubject.id,
        requests: Number(requests),
        window_seconds: Number(seconds),
      })
    } catch {
      return
    }
  }

  return (
    <form className="flex flex-col gap-5" onSubmit={(event) => void submit(event)}>
      <FieldGroup className="grid sm:grid-cols-2">
        <Field>
          <FieldLabel htmlFor={`${id}-requests`}>{t("access.rateLimits.requests")}</FieldLabel>
          <Input id={`${id}-requests`} type="number" min={1} step={1} value={requests} required onChange={(event) => setRequests(event.target.value)} />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-seconds`}>{t("access.rateLimits.windowSeconds")}</FieldLabel>
          <Input id={`${id}-seconds`} type="number" min={1} step={1} value={seconds} required onChange={(event) => setSeconds(event.target.value)} />
        </Field>
      </FieldGroup>
      <Button className="self-start" disabled={props.pending || !requests || !seconds}>{t("access.rateLimits.add")}</Button>
    </form>
  )
}
