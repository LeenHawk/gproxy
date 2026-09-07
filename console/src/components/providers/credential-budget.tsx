import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useId, useState, type FormEvent, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { toast } from "sonner"
import type { QuotaDto } from "@/generated/QuotaDto"
import type { QuotaWriteRequest } from "@/generated/QuotaWriteRequest"
import { quotas, saveQuota } from "@/api/identity"
import { quotaWindows } from "@/api/observability"
import { QueryState } from "@/components/query-state"
import { WindowBar } from "@/components/window-bar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"

const periods = ["total", "monthly", "weekly", "daily"] as const

export function CredentialBudget({ credentialId }: { credentialId: number }) {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ["quotas"], queryFn: quotas })
  const windows = useQuery({
    queryKey: ["quota-windows", "credential", credentialId],
    queryFn: () => quotaWindows("credential", credentialId),
    refetchInterval: 15_000,
  })
  const quota = query.data?.find((item) => item.subject_kind === "credential" && item.subject_id === credentialId)
  const blocked = quota?.enabled && windows.data?.some((window) => Number(window.cost_used) >= Number(window.cost_limit))
  return (
    <Card size="sm">
      <QueryState loading={query.isPending} error={query.isError ? t("access.quotas.loadError") : ""}>
        <BudgetForm key={JSON.stringify(quota) ?? credentialId} credentialId={credentialId} quota={quota} blocked={blocked}>
          <QueryState loading={windows.isPending} error={windows.isError ? t("access.quotas.loadError") : ""}>
            <div className="grid gap-4 sm:grid-cols-2">
              {windows.data?.map((window) => <WindowBar key={window.window_kind}
                label={t(`access.quotas.${window.window_kind}`)} used={window.cost_used} limit={window.cost_limit}
                end={window.reset_at} started={window.started}
                resetLabel={window.window_kind === "total" ? t("providers.credentials.budget.noReset") : undefined} />)}
            </div>
          </QueryState>
        </BudgetForm>
      </QueryState>
    </Card>
  )
}

function BudgetForm({ credentialId, quota, blocked, children }: { credentialId: number; quota?: QuotaDto; blocked?: boolean; children: ReactNode }) {
  const { t } = useTranslation()
  const id = useId()
  const client = useQueryClient()
  const [enabled, setEnabled] = useState(quota?.enabled ?? true)
  const [values, setValues] = useState({
    total: quota?.quota_total ?? "",
    monthly: quota?.quota_monthly ?? "",
    weekly: quota?.quota_weekly ?? "",
    daily: quota?.quota_daily ?? "",
  })
  const mutation = useMutation({
    mutationFn: (value: QuotaWriteRequest) => saveQuota(value, quota?.id),
    onSuccess: async () => {
      await Promise.all([
        client.invalidateQueries({ queryKey: ["quotas"] }),
        client.invalidateQueries({ queryKey: ["quota-windows"] }),
      ])
      toast.success(t("access.quotas.saved"))
    },
    onError: () => toast.error(t("access.quotas.saveError")),
  })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    mutation.mutate({
      subject_kind: "credential", subject_id: credentialId, enabled,
      quota_total: values.total || null,
      quota_monthly: values.monthly || null,
      quota_weekly: values.weekly || null,
      quota_daily: values.daily || null,
      quota_5h: quota?.quota_5h ?? null, quota_7d: quota?.quota_7d ?? null,
    })
  }
  return (
    <form onSubmit={submit} className="flex flex-col gap-4">
      <CardHeader>
        <div className="flex items-center justify-between gap-3">
          <CardTitle className="flex items-center gap-2">
            {t("providers.credentials.budget.title")}
            {blocked ? <Badge variant="destructive">{t("providers.credentials.budget.blocked")}</Badge> : null}
          </CardTitle>
          <div className="flex shrink-0 items-center gap-2">
            <Switch id={`${id}-enabled`} checked={enabled} onCheckedChange={setEnabled} aria-label={t("providers.credentials.budget.enabled")} />
            <FieldLabel htmlFor={`${id}-enabled`}>{t("common.actions.enable")}</FieldLabel>
            <Button type="submit" size="sm" disabled={mutation.isPending}>{t("common.actions.save")}</Button>
          </div>
        </div>
        <CardDescription>{t("providers.credentials.budget.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <FieldGroup className="grid sm:grid-cols-2 lg:grid-cols-4">
          {periods.map((period) => <Field key={period}>
            <FieldLabel htmlFor={`${id}-${period}`}>{t(`access.quotas.${period}`)}</FieldLabel>
            <Input id={`${id}-${period}`} type="number" inputMode="decimal" min="0" step="any"
              placeholder={t("providers.credentials.budget.unlimited")} value={values[period]}
              onChange={(event) => setValues({ ...values, [period]: event.target.value })} />
          </Field>)}
        </FieldGroup>
        {children}
      </CardContent>
    </form>
  )
}
