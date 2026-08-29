import { useMutation } from "@tanstack/react-query"
import { LoaderCircleIcon, WifiIcon } from "lucide-react"
import { useState } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { testConnectivity } from "@/api/control"
import type { ConnectivityProbeDto } from "@/generated/ConnectivityProbeDto"
import type { ConnectivityTestRequest } from "@/generated/ConnectivityTestRequest"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"

export function ConnectivityTest({ request, label, disabled = false, showLabel = false }: { request: ConnectivityTestRequest; label: string; disabled?: boolean; showLabel?: boolean }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const mutation = useMutation({
    mutationFn: () => testConnectivity(request),
    onSuccess: () => setOpen(true),
    onError: () => toast.error(t("connectivity.requestError")),
  })
  const icon = mutation.isPending ? <LoaderCircleIcon className="animate-spin" aria-hidden /> : <WifiIcon aria-hidden />
  return <>
    {/* The probe runs for up to 30 seconds, so a wordless spinner reads as nothing happening. */}
    <Button type="button" size={showLabel ? "sm" : "icon-sm"} variant="outline" disabled={disabled || mutation.isPending} aria-label={showLabel ? undefined : `${t("connectivity.action")}: ${label}`} onClick={(event) => { event.stopPropagation(); mutation.mutate() }}>
      {icon}
      {showLabel ? <span>{t(mutation.isPending ? "connectivity.testing" : "connectivity.action")}</span> : null}
    </Button>
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent closeLabel={t("common.actions.close")}>
        <DialogHeader><DialogTitle>{t("connectivity.title")}</DialogTitle><DialogDescription>{label}</DialogDescription></DialogHeader>
        <DialogBody className="flex flex-col gap-4">
          {mutation.data?.ok ? <>
            <Alert><AlertTitle>{t("connectivity.success")}</AlertTitle><AlertDescription>{t("connectivity.successHint")}</AlertDescription></Alert>
            <div className="grid gap-3 sm:grid-cols-2">
              {mutation.data.ipv4 ? <Probe version="4" value={mutation.data.ipv4} /> : null}
              {mutation.data.ipv6 ? <Probe version="6" value={mutation.data.ipv6} /> : null}
            </div>
          </> : <Alert variant="destructive"><AlertTitle>{t("connectivity.failed")}</AlertTitle><AlertDescription>{t(`connectivity.errors.${mutation.data?.error_code ?? "unknown"}`)}</AlertDescription></Alert>}
          {mutation.data ? <p className="text-xs text-muted-foreground">{t("connectivity.route", { source: t(`connectivity.sources.${mutation.data.proxy_source}`), latency: mutation.data.latency_ms })}</p> : null}
        </DialogBody>
      </DialogContent>
    </Dialog>
  </>
}

function Probe({ version, value }: { version: "4" | "6"; value: ConnectivityProbeDto }) {
  const { t } = useTranslation()
  return <div className="rounded-lg border p-3">
    <div className="mb-2 flex items-center justify-between"><Badge variant="outline">IPv{version}</Badge><span className="text-xs text-muted-foreground">{t("connectivity.latency", { value: value.latency_ms })}</span></div>
    <p className="break-all font-mono text-sm font-medium">{value.ip}</p>
    <p className="mt-1 text-xs text-muted-foreground">{[value.location, value.colo].filter(Boolean).join(" · ") || t("common.none")}</p>
  </div>
}
