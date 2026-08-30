import { useState, type FormEvent } from "react"
import { useTranslation } from "react-i18next"
import { LoaderCircleIcon } from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { LocaleControls } from "@/components/locale-controls"

export function AuthPanel({ setup, pending, failed, onSubmit }: { setup: boolean; pending: boolean; failed: boolean; onSubmit: (username: string, password: string) => void }) {
  const { t } = useTranslation()
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")

  const submit = (event: FormEvent) => {
    event.preventDefault()
    onSubmit(username.trim(), password)
  }

  return (
    <main className="grid min-h-screen place-items-center px-5 py-10">
      <div className="flex w-full max-w-md flex-col gap-4">
        <div className="flex justify-end"><LocaleControls /></div>
        <Card>
          <CardHeader>
            <p className="font-mono text-xs text-muted-foreground">{t("common.product")}</p>
            <CardTitle headingLevel={1}>{t(setup ? "auth.setup.title" : "auth.login.title")}</CardTitle>
            <CardDescription>{t(setup ? "auth.setup.description" : "auth.login.description")}</CardDescription>
          </CardHeader>
          <form onSubmit={submit}>
            <CardContent>
              <FieldGroup className="!grid-cols-1">
                <Field>
                  <FieldLabel htmlFor="username">{t("auth.username")}</FieldLabel>
                  <Input id="username" autoComplete="username" value={username} onChange={(event) => setUsername(event.target.value)} required />
                </Field>
                <Field>
                  <FieldLabel htmlFor="password">{t("auth.password")}</FieldLabel>
                  <Input id="password" type="password" autoComplete={setup ? "new-password" : "current-password"} value={password} onChange={(event) => setPassword(event.target.value)} required />
                </Field>
                {failed ? (
                  <Alert variant="destructive">
                    <AlertTitle>{t("auth.error.title")}</AlertTitle>
                    <AlertDescription>{t("auth.error.description")}</AlertDescription>
                  </Alert>
                ) : null}
              </FieldGroup>
            </CardContent>
            <CardFooter>
              <Button className="w-full" disabled={pending || !username.trim() || !password}>
                {pending ? <LoaderCircleIcon data-icon="inline-start" className="animate-spin" /> : null}
                {t(setup ? "auth.setup.action" : "auth.login.action")}
              </Button>
            </CardFooter>
          </form>
        </Card>
      </div>
    </main>
  )
}
