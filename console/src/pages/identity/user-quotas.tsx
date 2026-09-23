import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import { Plus, RefreshCw } from "lucide-react"
import { createUserQuota, deleteUserQuota, resetUserQuota, updateUserQuota, userBudgetStatus, userQuotaKey, userQuotas } from "@/api/user-quotas"
import type { QuotaDto } from "@/generated/sdk"
import { ConfirmButton } from "@/components/confirm"
import { RecordDialog, type FormField } from "@/components/record-form"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { formatInstant } from "@/lib/format"

const PERIODS = ["5h", "1d", "7d", "1m", "total", "custom"]

export function UserQuotasDialog({ userId, userName, onClose }: { userId: string; userName: string; onClose: () => void }) {
  const { t, i18n } = useTranslation()
  const client = useQueryClient()
  const key = userQuotaKey(userId)
  const [editing, setEditing] = useState<QuotaDto | "new" | null>(null)
  const list = useQuery({ queryKey: [...key, "rules"], queryFn: () => userQuotas(userId) })
  const status = useQuery({ queryKey: [...key, "status"], queryFn: () => userBudgetStatus(userId), refetchInterval: 15_000 })
  const invalidate = async () => {
    await Promise.all([
      client.invalidateQueries({ queryKey: key }),
      client.invalidateQueries({ queryKey: ["portal", "quota"] }),
    ])
  }
  const saved = useMutation({
    mutationFn: (body: Record<string, unknown>) => editing && editing !== "new"
      ? updateUserQuota(editing.id, body)
      : createUserQuota(userId, body),
    onSuccess: async () => { setEditing(null); await invalidate(); toast.success(t("toast.saved")) },
  })
  const action = useMutation({
    mutationFn: ({ row, kind }: { row: QuotaDto; kind: "toggle" | "delete" | "reset" }) => {
      if (kind === "delete") return deleteUserQuota(row.id)
      if (kind === "reset") return resetUserQuota(row.id)
      return updateUserQuota(row.id, { enabled: !row.enabled })
    },
    onSuccess: async () => { await invalidate(); toast.success(t("toast.saved")) },
  })
  const original = editing && editing !== "new" ? editing : undefined
  const periods = original && !PERIODS.includes(original.period) ? [...PERIODS, original.period] : PERIODS
  const fields: FormField[] = [
    { name: "windowKey", label: t("userQuota.name"), kind: "text", required: true },
    { name: "limitValue", label: t("userQuota.limit", { unit: original?.unit ?? "USD" }), kind: "text", required: true },
    { name: "period", kind: "select", required: true, choices: periods.map((value) => ({ value, label: PERIODS.includes(value) ? t(`userQuota.periods.${value}`) : value })) },
    { name: "periodSeconds", kind: "number", nullable: true },
    { name: "anchorAtMs", kind: "datetime", nullable: true },
    { name: "modelPattern", kind: "text", nullable: true },
    { name: "enabled", kind: "switch" },
  ]
  return (
    <Dialog open onOpenChange={(open) => { if (!open && !saved.isPending && !action.isPending) onClose() }}>
      <DialogContent className="sm:max-w-3xl" aria-describedby={undefined} closeLabel={t("actions.close")}>
        <DialogHeader>
          <DialogTitle>{t("userQuota.title", { name: userName })}</DialogTitle>
        </DialogHeader>
        <DialogBody>
          <div className="mb-4 flex flex-wrap gap-2">
            <Button size="sm" disabled={action.isPending} onClick={() => { saved.reset(); setEditing("new") }}><Plus data-icon="inline-start" />{t("userQuota.add")}</Button>
            <Button variant="outline" size="sm" disabled={list.isFetching || status.isFetching} onClick={() => void invalidate()}><RefreshCw data-icon="inline-start" />{t("actions.refresh")}</Button>
          </div>
          {action.error || status.error ? <ErrorNotice error={action.error ?? status.error} /> : null}
          <QueryState isPending={list.isPending} error={list.error}>
            {!list.data?.length ? <EmptyNotice title={t("userQuota.empty")} /> : (
              <div className="flex flex-col gap-3">
                {list.data.map((row) => {
                  const current = status.data?.find((item) => item.quotaId === row.id)
                  return (
                    <Card key={row.id}>
                      <CardHeader>
                        <CardTitle className="flex flex-wrap items-center gap-2">
                          <span className="break-all">{row.windowKey}</span>
                          <Badge variant={row.enabled ? "success" : "outline"}>{row.enabled ? t("userQuota.enabled") : t("userQuota.disabled")}</Badge>
                        </CardTitle>
                      </CardHeader>
                      <CardContent className="flex flex-col gap-3">
                        <p className="break-all text-sm">{t("userQuota.amount", { used: current?.used ?? "—", limit: row.limitValue, unit: row.unit })}</p>
                        <p className="text-sm text-muted-foreground">{PERIODS.includes(row.period) ? t(`userQuota.periods.${row.period}`) : row.period}{row.periodSeconds ? ` (${row.periodSeconds}s)` : ""} · {row.modelPattern || t("userQuota.allModels")}</p>
                        <p className="text-sm text-muted-foreground">{t("userQuota.resetAt", { at: current ? formatInstant(current.resetsAtMs, i18n.language) ?? t("userQuota.never") : "—" })}</p>
                        <div className="flex flex-wrap gap-1">
                          <Button variant="ghost" size="sm" disabled={action.isPending} onClick={() => { saved.reset(); setEditing(row) }}>{t("actions.edit")}</Button>
                          <Button variant="ghost" size="sm" disabled={action.isPending} onClick={() => action.mutate({ row, kind: "toggle" })}>{row.enabled ? t("userQuota.disable") : t("userQuota.enable")}</Button>
                          <ConfirmButton disabled={action.isPending || !row.enabled} title={t("userQuota.resetConfirm", { name: row.windowKey })} confirmLabel={t("userQuota.reset")} onConfirm={() => action.mutate({ row, kind: "reset" })}>{t("userQuota.reset")}</ConfirmButton>
                          <ConfirmButton disabled={action.isPending} title={t("userQuota.deleteConfirm", { name: row.windowKey })} onConfirm={() => action.mutate({ row, kind: "delete" })}>{t("actions.delete")}</ConfirmButton>
                        </div>
                      </CardContent>
                    </Card>
                  )
                })}
              </div>
            )}
          </QueryState>
        </DialogBody>
        <RecordDialog
          mode={original ? "edit" : "create"}
          open={editing !== null}
          onOpenChange={(open) => { if (!open && !saved.isPending) setEditing(null) }}
          title={original ? t("userQuota.edit") : t("userQuota.add")}
          fields={fields}
          original={original ? { ...original } : { period: "1m", enabled: true }}
          onSubmit={(body) => saved.mutate(body)}
          pending={saved.isPending}
          error={saved.error}
        />
      </DialogContent>
    </Dialog>
  )
}
