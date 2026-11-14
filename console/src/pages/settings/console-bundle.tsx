import { useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import {
  BUILTIN_CONSOLE_URL, CONSOLE_BUNDLE_KEY, CUSTOM_CONSOLE_URL,
  consoleBundleStatus, removeConsoleBundle, uploadConsoleBundle, type ConsoleBundleStatus,
} from "@/api/console-bundle"
import { ApiError } from "@/api/client"
import { ConfirmButton } from "@/components/confirm"
import { ErrorNotice } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { copyText } from "@/lib/copy-text"
import { formatInstant } from "@/lib/format"
import { saveTextFile } from "@/lib/save-file"
import agents from "./custom-console-agents.md?raw"

/** Settings > General: upload a console of the administrator's own, or go back to this one. */
export function ConsoleBundleSettings() {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const input = useRef<HTMLInputElement>(null)
  const [guide, setGuide] = useState(false)
  const status = useQuery({ queryKey: CONSOLE_BUNDLE_KEY, queryFn: consoleBundleStatus, retry: false })
  const onSuccess = (value: ConsoleBundleStatus) => client.setQueryData(CONSOLE_BUNDLE_KEY, value)
  const upload = useMutation({ mutationFn: uploadConsoleBundle, onSuccess: (value) => { onSuccess(value); toast.success(t("consoleBundle.uploaded")) } })
  const remove = useMutation({ mutationFn: removeConsoleBundle, onSuccess: (value) => { onSuccess(value); toast.success(t("consoleBundle.removed")) } })
  // The host mounts the routes only with the console switch on; a 404 means
  // this instance cannot take an upload, so there is nothing to offer.
  if (status.error instanceof ApiError && status.error.status === 404) return null
  const busy = upload.isPending || remove.isPending
  const error = upload.error ?? remove.error ?? status.error
  const data = status.data
  return <Card size="sm">
    <CardHeader>
      <CardTitle>{t("consoleBundle.title")}</CardTitle>
      <CardDescription>{t("consoleBundle.description")}</CardDescription>
    </CardHeader>
    <CardContent className="flex flex-col gap-3 text-sm">
      <p role="status">{status.isPending ? t("consoleBundle.loading") : data?.installed
        ? t("consoleBundle.installed", {
          time: formatInstant(data.installedAt == null ? null : data.installedAt * 1000, i18n.language) ?? "-",
          files: data.files,
          size: (data.bytes / 1024 / 1024).toFixed(1),
        })
        : t("consoleBundle.builtin")}</p>
      {data?.builtin ? <p className="text-muted-foreground">{t("consoleBundle.recovery")} <code>{BUILTIN_CONSOLE_URL}</code></p> : null}
      {error && !(error instanceof ApiError && error.status === 404) ? <ErrorNotice error={error} /> : null}
    </CardContent>
    <CardFooter className="flex flex-wrap gap-2">
      <input ref={input} type="file" accept=".zip,application/zip" hidden aria-label={t("consoleBundle.upload")} onChange={(event) => {
        const file = event.target.files?.[0]
        event.target.value = ""
        if (file) { remove.reset(); upload.mutate(file) }
      }} />
      <Button type="button" variant="outline" disabled={busy || !data} onClick={() => input.current?.click()}>
        {data?.installed ? t("consoleBundle.replace") : t("consoleBundle.upload")}
      </Button>
      {data?.installed ? <>
        <Button type="button" variant="outline" disabled={busy} onClick={() => window.location.assign(CUSTOM_CONSOLE_URL)}>{t("consoleBundle.open")}</Button>
        <ConfirmButton variant="outline" title={t("consoleBundle.removeConfirm")} confirmLabel={t("consoleBundle.remove")} disabled={busy}
          onConfirm={() => { upload.reset(); remove.mutate() }}>{t("consoleBundle.remove")}</ConfirmButton>
      </> : null}
      <Button type="button" variant="ghost" onClick={() => setGuide(true)}>{t("consoleBundle.guide")}</Button>
    </CardFooter>
    {guide ? <AgentsGuide onClose={() => setGuide(false)} /> : null}
  </Card>
}

function AgentsGuide({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  return <Dialog open onOpenChange={(open) => { if (!open) onClose() }}><DialogContent className="sm:max-w-3xl">
    <DialogHeader>
      <DialogTitle>AGENTS.md</DialogTitle>
      <DialogDescription>{t("consoleBundle.guideHelp")}</DialogDescription>
    </DialogHeader>
    <DialogBody>
      <pre className="max-h-[60vh] overflow-auto whitespace-pre-wrap rounded-md border bg-muted/40 p-3 font-mono text-xs">{agents}</pre>
    </DialogBody>
    <DialogFooter>
      <Button type="button" variant="outline" onClick={() => { void copyText(agents).then(() => toast.success(t("consoleBundle.copied"))) }}>{t("consoleBundle.copy")}</Button>
      <Button type="button" onClick={() => saveTextFile("AGENTS.md", "text/markdown", agents)}>{t("consoleBundle.download")}</Button>
    </DialogFooter>
  </DialogContent></Dialog>
}
