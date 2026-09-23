import { useState } from "react"
import { useMutation } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { api, json } from "@/api/client"
import type { ConnectivityResultDto } from "@/generated/sdk"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
export type ProxySettings = { mode: "direct" } | { mode: "system" } | { mode: "explicit"; url: string } | null
export type ProxyScope = ({ scope: "global" } | { scope: "provider"; provider_id: string } | { scope: "credential"; credential_id: string }) & { parent?: boolean }
export function ProxyControl({ id, value, onChange, scope }: { id: string; value: ProxySettings; onChange: (proxy: ProxySettings) => void; scope: ProxyScope }) {
  const { t } = useTranslation()
  const inherit = scope.parent || scope.scope !== "global"
  return <div className="flex min-w-0 flex-col gap-3">
    <Select value={value?.mode ?? (inherit ? "inherit" : "direct")} onValueChange={mode => onChange(mode === "inherit" ? null : mode === "explicit" ? { mode, url: value?.mode === "explicit" ? value.url : "" } : { mode: mode as "direct" | "system" })}>
      <SelectTrigger id={id} className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>
        {inherit ? <SelectItem value="inherit">{t("proxy.inherit")}</SelectItem> : null}
        <SelectItem value="direct">{t("proxy.direct")}</SelectItem><SelectItem value="system">{t("proxy.system")}</SelectItem><SelectItem value="explicit">{t("proxy.explicit")}</SelectItem>
      </SelectGroup></SelectContent>
    </Select>
    {value?.mode === "explicit" ? <Field><FieldLabel htmlFor={`${id}-url`}>{t("proxy.url")}</FieldLabel><Input id={`${id}-url`} type="url" required placeholder="http://127.0.0.1:10808" value={value.url} onChange={e => onChange({ mode: "explicit", url: e.target.value })} /></Field> : null}
    <div><EgressTest scope={scope} proxy={value} disabled={value?.mode === "explicit" && !value.url.trim()} /></div>
  </div>
}
export function EgressTest({ scope, proxy, disabled }: { scope: ProxyScope; proxy?: ProxySettings; disabled?: boolean }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const probe = useMutation({ mutationFn: () => { const { parent, ...target } = scope; return api<ConnectivityResultDto>("/admin/api/connectivity/test", json("POST", { ...target, ...(proxy === undefined || (parent && proxy === null) ? {} : { proxy }) })) } })
  return <><Button type="button" size="sm" variant="outline" disabled={disabled || probe.isPending} onClick={e => { e.stopPropagation(); setOpen(true); probe.mutate() }}>{t(probe.isPending ? "proxy.testing" : "proxy.test")}</Button>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="sm:max-w-lg" closeLabel={t("actions.close")} aria-describedby={undefined}><DialogHeader><DialogTitle>{t("proxy.test")}</DialogTitle></DialogHeader><DialogBody className="flex flex-col gap-3">
      {probe.isPending ? <p>{t("proxy.testing")}</p> : null}{probe.error ? <ErrorNotice error={probe.error} /> : null}
      {probe.data ? <><p>{t(probe.data.ok ? "proxy.success" : "proxy.failed")} · {probe.data.latencyMs} ms</p><p>{t("proxy.source")}: {t(`proxy.sources.${scope.parent && proxy ? "custom" : probe.data.proxySource}`)}</p>
        <div className="grid gap-3 sm:grid-cols-2">{(["ipv4", "ipv6"] as const).map(version => { const result = probe.data![version]; return <div key={version} className="min-w-0 rounded-lg border p-3"><p>{version === "ipv4" ? "IPv4" : "IPv6"}</p>{result ? <><p className="break-all font-mono">{result.ip}</p><p>{[result.location, result.colo].filter(Boolean).join(" · ")}</p><p>{result.latencyMs} ms</p></> : <p className="break-words text-muted-foreground">{t(`proxy.errors.${probe.data![version === "ipv4" ? "ipv4Error" : "ipv6Error"] ?? "transport"}`)}</p>}</div> })}</div>
      </> : null}
    </DialogBody></DialogContent></Dialog></>
}
