import { useMutation } from "@tanstack/react-query"
import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { exchangeCookie } from "@/api/login"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"

type Props = { providerId: number; label: string; onDone: () => void }

export function CookieFlow({ providerId, label, onDone }: Props) {
  const { t } = useTranslation()
  const id = useId()
  const [cookie, setCookie] = useState("")
  const exchange = useMutation({
    mutationFn: () => exchangeCookie({
      provider_id: providerId,
      cookie: cookie.trim(),
      label: label.trim() || null,
    }),
    onSuccess: onDone,
  })

  return (
    <div className="flex flex-col gap-4">
      <Field>
        <FieldLabel htmlFor={`${id}-cookie`}>{t("providers.login.cookieLabel")}</FieldLabel>
        <Textarea
          id={`${id}-cookie`}
          className="machine-text min-h-24"
          value={cookie}
          onChange={(event) => setCookie(event.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
        <FieldDescription>{t("providers.login.cookieHint")}</FieldDescription>
      </Field>
      {exchange.isError ? (
        <Alert variant="destructive">
          <AlertTitle>{t("providers.login.errors.cookieTitle")}</AlertTitle>
          <AlertDescription>{t("providers.login.errors.cookieDescription")}</AlertDescription>
        </Alert>
      ) : null}
      <Button type="button" onClick={() => exchange.mutate()} disabled={!cookie.trim() || exchange.isPending}>
        {t(exchange.isPending ? "providers.login.completing" : "providers.login.complete")}
      </Button>
    </div>
  )
}
