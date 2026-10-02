import { useEffect, useEffectEvent, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Copy, ExternalLink } from "lucide-react"
import { toast } from "sonner"
import { copyText } from "@/lib/copy-text"
import { useTranslation } from "react-i18next"
import type { CredentialProviderDto } from "@/generated/app"
import type { AuthCodeStarted, DeviceStarted } from "@/generated/sdk"
import { startAuthCode, completeAuthCode, startDevice, pollDevice, exchangeCookie } from "@/api/credentials"
import { ManagementDialog } from "@/components/management-dialog"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { ownerColumns, useOwnerChoices } from "./ownership"

export function CredentialLoginDialog({ provider, onClose }: { provider: CredentialProviderDto; onClose: () => void }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const ownership = useOwnerChoices()
  const [owner, setOwner] = useState(ownership.defaultOwner)
  const [label, setLabel] = useState("")
  const modes = provider.loginModes.filter(mode => mode !== "api_key")
  const [mode, setMode] = useState(modes[0])
  const [cookie, setCookie] = useState("")
  const [callback, setCallback] = useState("")
  const [auth, setAuth] = useState<AuthCodeStarted | null>(null)
  const [device, setDevice] = useState<DeviceStarted | null>(null)
  const [polling, setPolling] = useState(false)
  const [pollError, setPollError] = useState<Error | null>(null)
  const [terminal, setTerminal] = useState<string | null>(null)
  const finish = async () => { setCookie(""); setCallback(""); await client.invalidateQueries({ queryKey: ["admin", "/credentials"] }); onClose() }
  const start = useMutation({ mutationFn: async () => {
    const request = { providerId: provider.id, label: label.trim() || null, owner: ownerColumns(owner) }
    if (mode === "authorization_code") { setAuth(await startAuthCode({ ...request, redirectUri: null })); return }
    if (mode === "device_code") { setDevice(await startDevice(request)); setPolling(true); return }
    await exchangeCookie({ ...request, cookie }); await finish()
  } })
  const complete = useMutation({ mutationFn: async () => { await completeAuthCode({ loginSessionId: auth!.loginSessionId, callbackUrl: auth!.redirectUri ? callback.trim() : null, code: auth!.redirectUri ? null : callback.trim(), state: auth!.redirectUri ? null : new URL(auth!.authorizeUrl).searchParams.get("state") }); await finish() } })
  const poll = useMutation({ mutationFn: pollDevice })
  const performPoll = useEffectEvent((id: string) => poll.mutateAsync(id))
  const closeAfterDevice = useEffectEvent(() => onClose())
  useEffect(() => {
    if (!device || !polling) return
    let disposed = false
    let timer: ReturnType<typeof setTimeout>
    const next = (seconds: number) => { timer = setTimeout(() => { void step() }, Math.max(1, seconds) * 1000) }
    const step = async () => {
      if (disposed) return
      if (device.expiresAtMs !== null && Date.now() >= device.expiresAtMs) { setTerminal("expired"); setPolling(false); return }
      try {
        const result = await performPoll(device.loginSessionId)
        if (disposed) return
        if (result.status === "pending") { next(result.intervalSecs); return }
        if (result.status === "ready") { await client.invalidateQueries({ queryKey: ["admin", "/credentials"] }); if (!disposed) closeAfterDevice(); return }
        setPolling(false)
        setTerminal(result.status)
      } catch (error) { if (!disposed) { setPollError(error instanceof Error ? error : new Error(String(error))); setPolling(false) } }
    }
    next(device.intervalSecs)
    return () => { disposed = true; clearTimeout(timer) }
  }, [device, polling, client])
  const busy = start.isPending || complete.isPending || poll.isPending
  const started = !!auth || !!device
  const authorizationUrl = auth?.authorizeUrl ?? (device ? device.verificationUriComplete ?? device.verificationUri : null)
  const copyAuthorizationUrl = async () => {
    if (!authorizationUrl) return
    try { await copyText(authorizationUrl); toast.success(t("keys.copied")) }
    catch { toast.error(t("keys.copyFailed")) }
  }
  return <ManagementDialog title={`${provider.displayName ?? provider.name} · ${t("management.loginAdd")}`} onClose={onClose} busy={busy} className="sm:max-w-lg">
    <FieldGroup className="sm:grid-cols-1">
      {!started ? <>
        <Field><FieldLabel htmlFor="login-mode">{t("management.loginMode")}</FieldLabel><Select value={mode} onValueChange={value => setMode(value as typeof mode)} disabled={busy}><SelectTrigger id="login-mode"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{modes.map(value => <SelectItem key={value} value={value}>{t(`management.${value}`)}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        <Field><FieldLabel htmlFor="login-label">{t("fields.label")}</FieldLabel><Input id="login-label" value={label} onChange={e => setLabel(e.target.value)} disabled={busy} /></Field>
        <Field><FieldLabel htmlFor="login-owner">{t("management.owner")}</FieldLabel><Select value={owner} onValueChange={setOwner} disabled={busy}><SelectTrigger id="login-owner"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{ownership.choices.map(entry => <SelectItem key={entry.value} value={entry.value}>{entry.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>
        {mode === "cookie" ? <Field><FieldLabel htmlFor="login-cookie">Cookie</FieldLabel><Input id="login-cookie" type="password" autoComplete="off" value={cookie} onChange={e => setCookie(e.target.value)} disabled={busy} /></Field> : null}
        <Button className="self-start" disabled={busy || (mode === "cookie" && !cookie.trim())} onClick={() => start.mutate()}>{t("management.startLogin")}</Button>
      </> : null}
      {authorizationUrl ? <Field>
        <FieldLabel htmlFor="login-authorization-url">{t("management.authorizationUrl")}</FieldLabel>
        <Input id="login-authorization-url" readOnly value={authorizationUrl} onFocus={event => event.currentTarget.select()} />
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => void copyAuthorizationUrl()}><Copy data-icon="inline-start" />{t("actions.copy")}</Button>
          <Button asChild variant="outline"><a href={authorizationUrl} target="_blank" rel="noopener noreferrer"><ExternalLink data-icon="inline-start" />{t("management.openAuthorization")}</a></Button>
        </div>
      </Field> : null}
      {auth ? <>
        <p className="text-sm text-muted-foreground">{t(auth.redirectUri ? "management.callbackHelp" : "management.authorizationCodeHelp")}</p>
        <Field><FieldLabel htmlFor="login-callback">{t(auth.redirectUri ? "management.callbackUrl" : "management.authorizationCode")}</FieldLabel><Textarea id="login-callback" autoComplete="off" value={callback} onChange={e => setCallback(e.target.value)} disabled={busy} /></Field>
        <Button disabled={busy || !callback.trim()} onClick={() => complete.mutate()}>{t("management.completeLogin")}</Button>
      </> : null}
      {device ? <>
        <code className="select-all break-all">{device.userCode}</code>
        <p>{t(terminal ? `management.${terminal}` : polling ? "management.waiting" : "management.pollStopped")}</p>
        {device.expiresAtMs ? <p>{t("fields.expiresAtMs")}: {new Date(device.expiresAtMs).toLocaleString()}</p> : null}
        {!polling && !terminal ? <Button onClick={() => { setPollError(null); setPolling(true) }}>{t("actions.refresh")}</Button> : null}
      </> : null}
    </FieldGroup>
    {start.error || complete.error || pollError || ownership.error ? <ErrorNotice error={start.error ?? complete.error ?? pollError ?? ownership.error} /> : null}
  </ManagementDialog>
}
