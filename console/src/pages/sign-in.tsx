//! The sign-in page, which is also the signed-out state of the whole console.
//!
//! There is one form and no account creation: v4 has no self-registration, and
//! the first administrator is minted by `gproxy bootstrap admin` on the
//! command line. A console that offered a "sign up" link would be promising
//! something no route answers.

import { useState, type FormEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { signIn } from "@/api/session"
import { SESSION_KEY } from "@/capability/session"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function SignInPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [name, setName] = useState("")
  const [password, setPassword] = useState("")

  const submit = useMutation({
    mutationFn: () => signIn(name, password),
    // The cookie is set by the response; refetching the context is what turns
    // this page into the shell.
    onSuccess: () => client.invalidateQueries({ queryKey: SESSION_KEY }),
  })

  const onSubmit = (event: FormEvent) => {
    event.preventDefault()
    if (!name.trim() || !password) return
    submit.mutate()
  }

  return (
    <main className="flex min-h-dvh items-center justify-center bg-background px-4 py-10 text-foreground">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>{t("signIn.title")}</CardTitle>
          <CardDescription>{t("signIn.description")}</CardDescription>
        </CardHeader>
        <CardContent>
          <form className="space-y-4" onSubmit={onSubmit}>
            {submit.error ? <ErrorNotice error={submit.error} /> : null}
            <Field>
              <FieldLabel htmlFor="sign-in-name">{t("fields.name")}</FieldLabel>
              <Input
                id="sign-in-name"
                autoComplete="username"
                autoFocus
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="sign-in-password">{t("fields.password")}</FieldLabel>
              <Input
                id="sign-in-password"
                type="password"
                autoComplete="current-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </Field>
            <Button type="submit" className="w-full" disabled={submit.isPending}>
              {t("actions.signIn")}
            </Button>
          </form>
        </CardContent>
      </Card>
    </main>
  )
}
