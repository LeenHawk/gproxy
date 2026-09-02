import type { AuthCodeStartResponse } from "@/generated/AuthCodeStartResponse"
import { useMutation } from "@tanstack/react-query"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { completeAuthcode, startAuthcode } from "@/api/login"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldError, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"
import { validateCallbackUrl } from "@/lib/oauth-callback"

type Props = {
  providerId: number
  label: string
  params: Record<string, string>
  codeOnly?: boolean
  disabled?: boolean
  onDone: () => void
}

export function AuthcodeFlow({ providerId, label, params, codeOnly = false, disabled, onDone }: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [session, setSession] = useState<AuthCodeStartResponse | null>(null)
  const [completion, setCompletion] = useState("")
  const [touched, setTouched] = useState(false)
  const start = useMutation({
    mutationFn: () => startAuthcode({ provider_id: providerId, params, redirect_uri: null }),
    onSuccess: (value) => {
      setSession(value)
      window.open(value.authorize_url, "_blank", "noopener,noreferrer")
    },
  })
  const complete = useMutation({
    mutationFn: () => completeAuthcode({
      login_session_id: session?.login_session_id ?? "",
      callback_url: codeOnly ? null : completion.trim(),
      code: codeOnly ? completion.trim() : null,
      label: label.trim() || null,
    }),
    onSuccess: onDone,
  })

  const valid = session !== null && (codeOnly
    ? completion.trim().length > 0
    : validateCallbackUrl(completion, session.authorize_url))

  if (!session) {
    return (
      <div className="flex flex-col gap-4">
        {start.isError ? <StepError step="start" /> : null}
        <Button type="button" onClick={() => start.mutate()} disabled={disabled || start.isPending}>
          {t(start.isPending ? "providers.login.starting" : "providers.login.start")}
        </Button>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <Button type="button" variant="outline" onClick={() => window.open(session.authorize_url, "_blank", "noopener,noreferrer")}>
        {t("providers.login.openAuthorize")}
      </Button>
      <Field data-invalid={touched && completion.trim() !== "" && !valid ? true : undefined}>
        <FieldLabel htmlFor={`${id}-callback`}>
          {t(codeOnly ? "providers.login.codeLabel" : "providers.login.callbackLabel")}
        </FieldLabel>
        <Textarea
          id={`${id}-callback`}
          className="machine-text"
          value={completion}
          onChange={(event) => setCompletion(event.target.value)}
          onBlur={() => setTouched(true)}
          autoComplete="off"
          spellCheck={false}
        />
        {touched && completion.trim() !== "" && !valid
          ? <FieldError>{t(codeOnly ? "providers.login.codeInvalid" : "providers.login.callbackInvalid")}</FieldError>
          : <FieldDescription>{t(codeOnly ? "providers.login.codeHint" : "providers.login.callbackHint")}</FieldDescription>}
      </Field>
      {complete.isError ? <StepError step="complete" /> : null}
      <Button type="button" onClick={() => complete.mutate()} disabled={!valid || complete.isPending}>
        {t(complete.isPending ? "providers.login.completing" : "providers.login.complete")}
      </Button>
    </div>
  )
}

function StepError({ step }: { step: "start" | "complete" }) {
  const { t } = useTranslation()
  return (
    <Alert variant="destructive">
      <AlertTitle>{t(`providers.login.errors.${step}Title`)}</AlertTitle>
      <AlertDescription>{t(`providers.login.errors.${step}Description`)}</AlertDescription>
    </Alert>
  )
}
