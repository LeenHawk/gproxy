import { useId, useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { decideOAuthAuthorization, decideOAuthDevice, oauthAuthorization, oauthConsent, oauthDeviceConsent } from "@/api/oauth"
import { QueryState } from "@/components/query-state"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function OAuthConsent({ authorization, deviceCode }: { authorization: string | null; deviceCode: string | null }) {
  const { t } = useTranslation()
  const id = useId()
  const [code, setCode] = useState(deviceCode ?? "")
  const [submittedCode, setSubmittedCode] = useState(deviceCode ?? "")
  const [finished, setFinished] = useState<boolean | null>(null)
  const query = useQuery({
    queryKey: ["portal", "oauth-consent", authorization, submittedCode],
    queryFn: ({ signal }) => authorization != null ? oauthConsent(authorization, signal) : oauthDeviceConsent(submittedCode, signal),
    enabled: authorization != null || submittedCode.length > 0,
    retry: false,
  })
  const decision = useMutation({
    mutationFn: async (approved: boolean) => {
      if (authorization != null) {
        const result = await decideOAuthAuthorization({ authorization: oauthAuthorization(authorization), approved })
        window.location.assign(result.redirect_uri)
      } else {
        await decideOAuthDevice({ user_code: submittedCode, approved })
        setFinished(approved)
      }
    },
  })
  return (
    <Card className="mx-auto w-full max-w-xl">
      <CardHeader><CardTitle>{t("portal.consent.title")}</CardTitle><CardDescription>{t("portal.consent.description")}</CardDescription></CardHeader>
      <CardContent className="flex flex-col gap-4">
        {finished != null ? <><Alert><AlertTitle>{t(finished ? "portal.consent.approved" : "portal.consent.denied")}</AlertTitle><AlertDescription>{t("portal.consent.returnToClient")}</AlertDescription></Alert><Button asChild variant="outline"><a href="/portal">{t("portal.consent.account")}</a></Button></> : (
          <>
            {authorization == null ? (
              <form className="flex flex-col gap-3" onSubmit={(event) => { event.preventDefault(); setSubmittedCode(code.trim()) }}>
                <FieldGroup><Field><FieldLabel htmlFor={id}>{t("portal.consent.deviceCode")}</FieldLabel><Input id={id} required maxLength={32} autoComplete="off" value={code} disabled={decision.isPending} onChange={(event) => setCode(event.target.value)} /></Field></FieldGroup>
                <Button type="submit" variant="outline" disabled={decision.isPending}>{t("portal.consent.lookup")}</Button>
              </form>
            ) : null}
            <QueryState loading={query.isLoading} error={query.isError ? t("portal.consent.loadError") : ""}>
              {query.data ? <>
                <dl className="flex flex-col gap-3">
                  <div><dt>{t("portal.consent.application")}</dt><dd>{query.data.client_name}</dd></div>
                  <div><dt>{t("portal.consent.clientId")}</dt><dd><code className="break-all">{query.data.client_id}</code></dd></div>
                  <div><dt>{t("portal.consent.user")}</dt><dd>{query.data.user_name}</dd></div>
                </dl>
                <Alert><AlertTitle>{t("portal.consent.permissions")}</AlertTitle><AlertDescription>{t("portal.consent.permissionsDescription")}</AlertDescription></Alert>
                <p className="text-sm text-muted-foreground">{t("portal.consent.identityWarning")}</p>
                {decision.isError ? <Alert variant="destructive"><AlertTitle>{t("portal.consent.saveError")}</AlertTitle></Alert> : null}
                <div className="flex flex-wrap gap-2"><Button disabled={decision.isPending} onClick={() => decision.mutate(true)}>{t("portal.consent.allow")}</Button><Button variant="outline" disabled={decision.isPending} onClick={() => decision.mutate(false)}>{t("portal.consent.deny")}</Button></div>
              </> : null}
            </QueryState>
          </>
        )}
      </CardContent>
    </Card>
  )
}
