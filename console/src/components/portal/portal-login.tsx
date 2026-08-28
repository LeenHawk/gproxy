import { useId, useState } from "react"
import { useTranslation } from "react-i18next"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function PortalLogin({
  pending,
  failed,
  onSubmit,
}: {
  pending: boolean
  failed: boolean
  onSubmit: (key: string) => void
}) {
  const { t } = useTranslation()
  const keyId = useId()
  const [key, setKey] = useState("")

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const value = key.trim()
    if (value) onSubmit(value)
  }

  return (
    <Card className="mx-auto w-full max-w-md">
      <CardHeader>
        <CardTitle headingLevel={1}>{t("portal.login.title")}</CardTitle>
        <CardDescription>{t("portal.login.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit}>
          <FieldGroup>
            {failed ? <Alert variant="destructive"><AlertTitle>{t("portal.login.error")}</AlertTitle></Alert> : null}
            <Field>
              <FieldLabel htmlFor={keyId}>{t("portal.login.key")}</FieldLabel>
              <Input
                id={keyId}
                type="password"
                value={key}
                required
                autoFocus
                autoComplete="off"
                autoCapitalize="none"
                spellCheck={false}
                onChange={(event) => setKey(event.target.value)}
              />
              <FieldDescription>{t("portal.login.keyHint")}</FieldDescription>
            </Field>
            <Button type="submit" disabled={pending || key.trim().length === 0}>
              {t(pending ? "portal.login.submitting" : "portal.login.action")}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}
