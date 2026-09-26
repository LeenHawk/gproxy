//! The two consent pages of the gateway's own OAuth issuer.
//!
//! Both render outside the shell: a person arrives here from another program —
//! a CLI that opened a browser, or a device showing a code — to answer one
//! question, and a sidebar full of administration would only be in the way.
//! The session gate still runs first, so a signed-out visitor signs in on this
//! same URL and lands back on the question.

import { useState, type FormEvent, type ReactNode } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import type { ConsentDecision } from "@/generated/app"
import { authorizeDecide, authorizeDetails, deviceDecide, deviceDetails } from "@/api/oauth"
import { ErrorNotice, LoadingRows } from "@/components/state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

function ConsentFrame({ title, description, children }: { title: string; description?: ReactNode; children: ReactNode }) {
  return (
    <main className="flex min-h-dvh items-center justify-center bg-background px-4 py-10 text-foreground">
      <Card className="w-full max-w-md">
        <CardHeader>
          <div className="mb-2 flex items-center gap-3">
            <img src={`${import.meta.env.BASE_URL}favicon-96x96.png`} alt="" className="size-10" />
            <span className="text-lg font-semibold">GPROXY</span>
          </div>
          <CardTitle>{title}</CardTitle>
          {description ? <CardDescription>{description}</CardDescription> : null}
        </CardHeader>
        <CardContent className="space-y-4">{children}</CardContent>
      </Card>
    </main>
  )
}

function Scopes({ scopes }: { scopes: string[] }) {
  const { t } = useTranslation()
  if (scopes.length === 0) return null
  return (
    <div className="space-y-2">
      <div className="text-sm text-muted-foreground">{t("oauthConsent.scopes")}</div>
      <div className="flex flex-wrap gap-1.5">
        {scopes.map(scope => <Badge key={scope} variant="secondary">{scope}</Badge>)}
      </div>
    </div>
  )
}

function Decide({ pending, onDecide }: { pending: boolean; onDecide: (decision: ConsentDecision) => void }) {
  const { t } = useTranslation()
  return (
    <div className="flex gap-2">
      <Button className="flex-1" disabled={pending} onClick={() => onDecide("approve")}>{t("oauthConsent.approve")}</Button>
      <Button className="flex-1" variant="outline" disabled={pending} onClick={() => onDecide("deny")}>{t("oauthConsent.deny")}</Button>
    </div>
  )
}

/** `/console/authorize?…`: a client asking to act as the signed-in person. */
export function AuthorizePage({ userName }: { userName: string }) {
  const { t } = useTranslation()
  // The issuer validates the query; the page only carries it back.
  const [search] = useState(() => window.location.search)
  const details = useQuery({ queryKey: ["oauth", "authorize", search], queryFn: () => authorizeDetails(search), retry: false })
  const decide = useMutation({
    mutationFn: (decision: ConsentDecision) => authorizeDecide(search, decision),
    // Back to the client in both directions: an approval carries the code, a
    // refusal carries `access_denied`.
    onSuccess: result => window.location.assign(result.location),
  })

  if (details.isPending) return <ConsentFrame title={t("oauthConsent.authorizeTitle")}><LoadingRows rows={3} /></ConsentFrame>
  if (details.error) {
    return (
      <ConsentFrame title={t("oauthConsent.authorizeTitle")} description={t("oauthConsent.invalidRequest")}>
        <ErrorNotice error={details.error} />
      </ConsentFrame>
    )
  }
  const request = details.data
  let destination = request.redirect_uri
  try { destination = new URL(request.redirect_uri).host || request.redirect_uri } catch { /* shown verbatim */ }
  return (
    <ConsentFrame
      title={t("oauthConsent.authorizeTitle")}
      description={t("oauthConsent.authorizeDescription", { client: request.client_name, user: userName })}
    >
      <Scopes scopes={request.scopes} />
      <p className="text-sm text-muted-foreground">{t("oauthConsent.redirectsTo", { destination })}</p>
      {decide.error ? <ErrorNotice error={decide.error} /> : null}
      <Decide pending={decide.isPending || decide.isSuccess} onDecide={decision => decide.mutate(decision)} />
    </ConsentFrame>
  )
}

/** `/console/device?user_code=…`: a device waiting for the code it showed to be approved. */
export function DevicePage({ userName }: { userName: string }) {
  const { t } = useTranslation()
  const [typed, setTyped] = useState(() => new URLSearchParams(window.location.search).get("user_code") ?? "")
  // Looked up only once the person says the code is right, not per keystroke.
  const [code, setCode] = useState(typed.trim())
  const details = useQuery({ queryKey: ["oauth", "device", code], queryFn: () => deviceDetails(code), enabled: code !== "", retry: false })
  const decide = useMutation({ mutationFn: (decision: ConsentDecision) => deviceDecide(code, decision) })

  if (decide.data) {
    return (
      <ConsentFrame
        title={decide.data.approved ? t("oauthConsent.deviceApproved") : t("oauthConsent.deviceDenied")}
        description={t("oauthConsent.returnToDevice")}
      >
        <div className="font-mono text-sm">{decide.data.user_code}</div>
      </ConsentFrame>
    )
  }

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setCode(typed.trim())
  }

  return (
    <ConsentFrame title={t("oauthConsent.deviceTitle")} description={t("oauthConsent.deviceDescription")}>
      <form className="flex items-end gap-2" onSubmit={submit}>
        <Field className="flex-1">
          <FieldLabel htmlFor="device-code">{t("oauthConsent.userCode")}</FieldLabel>
          <Input
            id="device-code"
            autoFocus={!code}
            autoComplete="off"
            spellCheck={false}
            className="font-mono uppercase"
            value={typed}
            onChange={event => setTyped(event.target.value)}
          />
        </Field>
        <Button type="submit" variant="outline" disabled={!typed.trim() || details.isFetching}>{t("oauthConsent.lookUp")}</Button>
      </form>
      {code && details.isPending ? <LoadingRows rows={2} /> : null}
      {details.error ? <ErrorNotice error={details.error} /> : null}
      {details.data ? (
        <>
          <p className="text-sm">{t("oauthConsent.authorizeDescription", { client: details.data.client_name, user: userName })}</p>
          <Scopes scopes={details.data.scopes} />
          {decide.error ? <ErrorNotice error={decide.error} /> : null}
          <Decide pending={decide.isPending} onDecide={decision => decide.mutate(decision)} />
        </>
      ) : null}
    </ConsentFrame>
  )
}
