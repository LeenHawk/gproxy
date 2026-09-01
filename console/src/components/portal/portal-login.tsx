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
  onSubmit: (username: string, password: string) => void
}) {
  const { t } = useTranslation()
  const usernameId = useId()
  const passwordId = useId()
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const value = username.trim()
    if (value && password) onSubmit(value, password)
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
              <FieldLabel htmlFor={usernameId}>{t("portal.login.username")}</FieldLabel>
              <Input
                id={usernameId}
                value={username}
                required
                autoFocus
                autoComplete="username"
                autoCapitalize="none"
                spellCheck={false}
                onChange={(event) => setUsername(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor={passwordId}>{t("portal.login.password")}</FieldLabel>
              <Input
                id={passwordId}
                type="password"
                value={password}
                required
                autoComplete="current-password"
                onChange={(event) => setPassword(event.target.value)}
              />
              <FieldDescription>{t("portal.login.passwordHint")}</FieldDescription>
            </Field>
            <Button type="submit" disabled={pending || username.trim().length === 0 || password.length === 0}>
              {t(pending ? "portal.login.submitting" : "portal.login.action")}
            </Button>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  )
}
