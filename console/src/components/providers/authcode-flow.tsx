import type { AuthCodeStartResponse } from "@/generated/AuthCodeStartResponse"
import { useMutation } from "@tanstack/react-query"
import { ExternalLinkIcon } from "lucide-react"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { completeAuthcode, startAuthcode } from "@/api/login"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"

type Props = {
  providerId: number
  label: string
  params: Record<string, string>
  disabled?: boolean
  onDone: () => void
}

export function AuthcodeFlow({ providerId, label, params, disabled, onDone }: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [session, setSession] = useState<AuthCodeStartResponse | null>(null)
  const [callbackUrl, setCallbackUrl] = useState("")
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
      callback_url: callbackUrl.trim(),
      label: label.trim() || null,
    }),
    onSuccess: onDone,
  })

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
        <ExternalLinkIcon data-icon="inline-start" />
        {t("providers.login.openAuthorize")}
      </Button>
      <Field>
        <FieldLabel htmlFor={`${id}-callback`}>{t("providers.login.callbackLabel")}</FieldLabel>
        <Textarea
          id={`${id}-callback`}
          className="machine-text min-h-24"
          value={callbackUrl}
          onChange={(event) => setCallbackUrl(event.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
        <FieldDescription>{t("providers.login.callbackHint")}</FieldDescription>
      </Field>
      {complete.isError ? <StepError step="complete" /> : null}
      <Button type="button" onClick={() => complete.mutate()} disabled={!callbackUrl.trim() || complete.isPending}>
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
