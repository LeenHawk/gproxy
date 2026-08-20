import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type Quota, type Scope, quotaQuery, upsertQuota, deleteQuota,
} from "@/api/authz";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { effectiveWindowUsed, type QuotaWindow } from "@/lib/quota-window";

const WINDOWS = [
  { window: "day", limit: "quota_daily", label: "dailyLimit" },
  { window: "week", limit: "quota_weekly", label: "weeklyLimit" },
  { window: "month", limit: "quota_monthly", label: "monthlyLimit" },
  { window: "five_hour", limit: "quota_5h", label: "fiveHourLimit" },
  { window: "seven_day", limit: "quota_7d", label: "sevenDayLimit" },
] as const satisfies ReadonlyArray<{ window: QuotaWindow; limit: keyof Quota; label: string }>;

interface QuotaFormValues {
  total: string;
  daily: string | null;
  weekly: string | null;
  monthly: string | null;
  fiveHour: string | null;
  sevenDay: string | null;
}

function isNumeric(value: string) {
  return value.trim() !== "" && Number.isFinite(Number(value.trim()));
}

function QuotaForm({
  quota, scope, scopeId, pending, onSave, onClear,
}: {
  quota: Quota | null | undefined;
  scope: Scope;
  scopeId: number;
  pending: boolean;
  onSave: (values: QuotaFormValues) => void;
  onClear: () => void;
}) {
  const { t } = useTranslation("identity");
  const [total, setTotal] = useState(quota?.quota_total ?? "");
  const [daily, setDaily] = useState(quota?.quota_daily ?? "");
  const [weekly, setWeekly] = useState(quota?.quota_weekly ?? "");
  const [monthly, setMonthly] = useState(quota?.quota_monthly ?? "");
  const [fiveHour, setFiveHour] = useState(quota?.quota_5h ?? "");
  const [sevenDay, setSevenDay] = useState(quota?.quota_7d ?? "");
  const windowsValid = [daily, weekly, monthly, fiveHour, sevenDay].every((value) => !value.trim() || isNumeric(value));
  const valid = isNumeric(total) && windowsValid;

  const optionalValue = (value: string) => value.trim() || null;

  return (
    <form
      className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!valid) return;
        onSave({
          total: total.trim(),
          daily: optionalValue(daily),
          weekly: optionalValue(weekly),
          monthly: optionalValue(monthly),
          fiveHour: optionalValue(fiveHour),
          sevenDay: optionalValue(sevenDay),
        });
      }}
    >
      <QuotaInput id={`q-total-${scope}-${scopeId}`} label={t("access.quotaTotal")} value={total} onChange={setTotal} placeholder="100.00" />
      <QuotaInput id={`q-day-${scope}-${scopeId}`} label={t("access.dailyLimit")} value={daily} onChange={setDaily} />
      <QuotaInput id={`q-week-${scope}-${scopeId}`} label={t("access.weeklyLimit")} value={weekly} onChange={setWeekly} />
      <QuotaInput id={`q-month-${scope}-${scopeId}`} label={t("access.monthlyLimit")} value={monthly} onChange={setMonthly} />
      <QuotaInput id={`q-5h-${scope}-${scopeId}`} label={t("access.fiveHourLimit")} value={fiveHour} onChange={setFiveHour} />
      <QuotaInput id={`q-7d-${scope}-${scopeId}`} label={t("access.sevenDayLimit")} value={sevenDay} onChange={setSevenDay} />
      <div className="flex items-center gap-2 sm:col-span-2 xl:col-span-4">
        <Button type="submit" disabled={pending || !valid}>{t("access.setQuota")}</Button>
        {quota && (
          <Button type="button" variant="ghost" className="text-destructive" onClick={onClear}>
            {t("access.clearQuota")}
          </Button>
        )}
      </div>
    </form>
  );
}

function QuotaInput({
  id, label, value, onChange, placeholder,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <div className="grid gap-1">
      <Label htmlFor={id} className="text-xs">{label}</Label>
      <Input id={id} inputMode="decimal" value={value} onChange={(event) => onChange(event.target.value)} placeholder={placeholder} />
    </div>
  );
}

export function QuotaSection({ scope, scopeId }: { scope: Scope; scopeId: number }) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const qc = useQueryClient();
  const key = ["quotas", scope, scopeId];
  const { data: quota } = useQuery(quotaQuery(scope, scopeId));
  const [confirmClear, setConfirmClear] = useState(false);

  const save = useMutation({
    mutationFn: async (values: QuotaFormValues) => {
      // Re-fetch the freshest quota immediately before upsert: cost_used is a billing-owned
      // accumulator the server increments — sending a stale cached value would overwrite/erase
      // accumulated spend (the backend writes the input cost_used directly on update).
      const fresh = await qc.fetchQuery(quotaQuery(scope, scopeId));
      return upsertQuota({
        id: fresh?.id ?? quota?.id ?? null,
        scope,
        scope_id: scopeId,
        quota_total: values.total,
        quota_daily: values.daily,
        quota_weekly: values.weekly,
        quota_monthly: values.monthly,
        quota_5h: values.fiveHour,
        quota_7d: values.sevenDay,
        cost_used: fresh?.cost_used ?? "0",
      });
    },
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); toast.success(tc("actions.save")); },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  const removal = useMutation({
    mutationFn: () => { if (!quota) return Promise.resolve(); return deleteQuota(quota.id); },
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setConfirmClear(false); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setConfirmClear(false); },
  });

  return (
    <section className="grid gap-2">
      <div>
        <h3 className="text-sm font-medium">{t("access.quota")}</h3>
        <p className="text-xs text-muted-foreground">{t("access.quotaHint")}</p>
      </div>
      {quota ? (
        <div className="grid gap-1 text-sm text-muted-foreground">
          <p>
            {t("access.costUsed")}: <span className="font-mono">{quota.cost_used}</span> / <span className="font-mono">{quota.quota_total}</span>
          </p>
          {WINDOWS.map(({ window, limit, label }) => quota[limit] !== null && (
            <p key={window}>
              {t(`access.${label}`)}: <span className="font-mono">{effectiveWindowUsed(quota, window)}</span> / <span className="font-mono">{quota[limit]}</span>
            </p>
          ))}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">{t("access.noQuota")}</p>
      )}
      <p className="text-xs text-muted-foreground">{t("access.windowQuotaHint")}</p>
      <QuotaForm
        key={`${scope}-${scopeId}-${quota?.id ?? "new"}-${quota?.updated_at ?? ""}`}
        quota={quota}
        scope={scope}
        scopeId={scopeId}
        pending={save.isPending}
        onSave={(values) => save.mutate(values)}
        onClear={() => setConfirmClear(true)}
      />
      <ConfirmDangerous
        open={confirmClear}
        onOpenChange={(o) => { if (!o) setConfirmClear(false); }}
        title={t("access.clearQuota")}
        description={t("access.deleteQuotaConfirm")}
        confirmLabel={t("access.clearQuota")}
        onConfirm={() => removal.mutate()}
        pending={removal.isPending}
      />
    </section>
  );
}
