import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { CheckCircle2Icon, LoaderCircleIcon, RefreshCwIcon, RotateCcwIcon, TriangleAlertIcon } from "lucide-react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { applyUpdate, rollbackUpdate, updateStatus } from "@/api/native"
import { ConfirmDangerous } from "@/components/confirm-dangerous"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"

export function UpdatePanel() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [confirm, setConfirm] = useState<"apply" | "rollback" | null>(null)
  const [notesOpen, setNotesOpen] = useState(false)
  const query = useQuery({
    queryKey: ["native-update"],
    queryFn: updateStatus,
    enabled: false,
    retry: false,
    staleTime: 0,
  })
  const apply = useMutation({
    mutationFn: applyUpdate,
    onSuccess: async (result) => {
      setConfirm(null)
      if (result.restart === "none") {
        toast.success(t("update.actions.applied", { version: result.version }))
        await queryClient.invalidateQueries({ queryKey: ["native-update"] })
      } else {
        toast.success(t("update.actions.restarting", { version: result.version }))
      }
    },
    onError: () => toast.error(t("update.actions.applyError")),
  })
  const rollback = useMutation({
    mutationFn: rollbackUpdate,
    onSuccess: (result) => {
      setConfirm(null)
      toast.success(t("update.actions.rolledBack", { version: result.version }))
    },
    onError: () => toast.error(t("update.actions.rollbackError")),
  })
  const data = query.data
  const busy = query.isFetching || apply.isPending || rollback.isPending

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>{t("update.check.title")}</CardTitle>
          <CardDescription>{t("update.check.description")}</CardDescription>
          <CardAction>
            <Button variant="outline" disabled={busy} onClick={() => void query.refetch()}>
              {query.isFetching
                ? <LoaderCircleIcon data-icon="inline-start" className="animate-spin" aria-hidden />
                : <RefreshCwIcon data-icon="inline-start" aria-hidden />}
              {t("update.check.button")}
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {query.isError ? (
            <Alert variant="destructive">
              <TriangleAlertIcon aria-hidden />
              <AlertTitle>{t("update.check.error")}</AlertTitle>
              <AlertDescription>{query.error.message}</AlertDescription>
            </Alert>
          ) : data ? (
            <div className="flex flex-col gap-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <Version label={t("update.check.current")} value={data.current} />
                <Version label={t("update.check.latest")} value={data.latest} />
              </div>
              <dl className="grid gap-2 text-sm">
                <Detail label={t("update.check.channel")} value={t(`update.preferences.channels.${data.channel}`)} />
                <Detail label={t("update.check.target")} value={data.target} mono />
                <Detail label={t("update.check.restart")} value={t(`update.restart.${data.restart}`)} />
              </dl>
              <Alert>
                <CheckCircle2Icon aria-hidden />
                <AlertTitle>{data.available ? t("update.check.available") : t("update.check.currentStatus")}</AlertTitle>
                <AlertDescription>{data.available ? t("update.check.availableHint", { version: data.latest }) : t("update.check.currentHint")}</AlertDescription>
              </Alert>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">{t("update.check.idle")}</p>
          )}
        </CardContent>
        {data ? (
          <CardFooter className="flex flex-wrap justify-end gap-2">
            {data.notes ? (
              <Button variant="ghost" onClick={() => setNotesOpen(true)}>
                {t("update.notes.action")}
              </Button>
            ) : null}
            <Button variant="outline" disabled={!data.rollback_available || busy} onClick={() => setConfirm("rollback")}>
              <RotateCcwIcon data-icon="inline-start" aria-hidden />
              {t("update.actions.rollback")}
            </Button>
            <Button disabled={!data.available || busy} onClick={() => setConfirm("apply")}>
              {t(data.available
                ? data.restart === "none" ? "update.actions.apply" : "update.actions.applyAndRestart"
                : "update.actions.current")}
            </Button>
          </CardFooter>
        ) : null}
      </Card>

      <Dialog open={notesOpen} onOpenChange={setNotesOpen}>
        <DialogContent className="sm:max-w-2xl" closeLabel={t("common.actions.close")}>
          <DialogHeader>
            <DialogTitle>{t("update.notes.title")}</DialogTitle>
            {data ? (
              <DialogDescription>{t("update.notes.description", { current: data.current, latest: data.latest })}</DialogDescription>
            ) : null}
          </DialogHeader>
          <DialogBody>
            <pre className="whitespace-pre-wrap font-sans text-sm leading-relaxed">{data?.notes}</pre>
          </DialogBody>
        </DialogContent>
      </Dialog>

      <ConfirmDangerous
        open={confirm === "apply"}
        onOpenChange={(open) => setConfirm(open ? "apply" : null)}
        title={t(data?.restart === "none" ? "update.actions.applyConfirmTitle" : "update.actions.restartConfirmTitle")}
        description={t(data?.restart === "none" ? "update.actions.applyConfirmDescription" : "update.actions.restartConfirmDescription")}
        confirmLabel={t(data?.restart === "none" ? "update.actions.apply" : "update.actions.applyAndRestart")}
        pending={apply.isPending}
        onConfirm={() => apply.mutate()}
      />
      <ConfirmDangerous
        open={confirm === "rollback"}
        onOpenChange={(open) => setConfirm(open ? "rollback" : null)}
        title={t("update.actions.rollbackConfirmTitle")}
        description={t("update.actions.rollbackConfirmDescription")}
        confirmLabel={t("update.actions.rollback")}
        pending={rollback.isPending}
        onConfirm={() => rollback.mutate()}
      />
    </>
  )
}

function Version({ label, value }: { label: string; value: string }) {
  return <div className="flex items-center justify-between gap-3 rounded-lg border p-3"><span className="text-muted-foreground">{label}</span><Badge variant="outline" className="font-mono">{value}</Badge></div>
}

function Detail({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="flex flex-wrap justify-between gap-2"><dt className="text-muted-foreground">{label}</dt><dd className={mono ? "font-mono text-xs" : undefined}>{value}</dd></div>
}
