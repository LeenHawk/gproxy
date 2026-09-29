import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { api } from "@/api/client"
import { copyText } from "@/lib/copy-text"
import type { QuotaProbeDto } from "@/generated/sdk"
import { Button } from "@/components/ui/button"
import { ErrorNotice } from "@/components/state"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { useState } from "react"

export function QuotaDiagnostics({ id, disabled }: { id: string; disabled: boolean }) {
  const { t } = useTranslation(), client = useQueryClient()
  const [open, setOpen] = useState(false)
  const probe = useMutation({ mutationFn: () => api<QuotaProbeDto>(`/admin/api/credentials/${encodeURIComponent(id)}/quota-diagnostics`, { method: "POST" }), onSuccess: async data => {
    if (data.snapshot) client.setQueryData(["credential-quota-probe", id], data.snapshot)
    await client.invalidateQueries({ queryKey: ["credential-quota", id] })
  } })
  const raw = JSON.stringify(probe.data?.responses, null, 2)
  return <>
    <Button variant="outline" disabled={disabled || probe.isPending} onClick={() => { setOpen(true); probe.mutate() }}>{t("limits.rawResponse")}</Button>
    <Dialog open={open} onOpenChange={setOpen}><DialogContent className="sm:max-w-3xl">
      <DialogHeader><DialogTitle>{t("limits.rawResponse")}</DialogTitle><DialogDescription>{t("limits.rawResponseHelp")}</DialogDescription></DialogHeader>
      <DialogBody className="flex flex-col gap-3">
        {probe.isPending ? <p role="status">{t("state.loading")}</p> : null}
        {probe.error || probe.data?.error ? <ErrorNotice error={probe.error ?? new Error(probe.data!.error!)} /> : null}
        {probe.data ? <><Button variant="outline" className="self-end" onClick={async () => { try { await copyText(raw); toast.success(t("actions.copied")) } catch (error) { toast.error(String(error)) } }}>{t("actions.copy")}</Button><pre className="max-h-[55vh] overflow-auto whitespace-pre-wrap break-all text-xs">{raw}</pre></> : null}
      </DialogBody>
    </DialogContent></Dialog>
  </>
}
