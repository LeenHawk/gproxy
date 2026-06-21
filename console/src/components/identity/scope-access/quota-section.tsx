import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type Scope, quotaQuery, upsertQuota, deleteQuota,
} from "@/api/authz";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function QuotaSection({ scope, scopeId }: { scope: Scope; scopeId: number }) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const qc = useQueryClient();
  const key = ["quotas", scope, scopeId];
  const { data: quota } = useQuery(quotaQuery(scope, scopeId));
  const [total, setTotal] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);

  const isNumeric = (v: string) => v.trim() !== "" && Number.isFinite(Number(v.trim()));

  const save = useMutation({
    mutationFn: async () => {
      // Re-fetch the freshest quota immediately before upsert: cost_used is a billing-owned
      // accumulator the server increments — sending a stale cached value would overwrite/erase
      // accumulated spend (the backend writes the input cost_used directly on update).
      const fresh = await qc.fetchQuery(quotaQuery(scope, scopeId));
      return upsertQuota({ id: fresh?.id ?? quota?.id ?? null, scope, scope_id: scopeId, quota_total: total.trim(), cost_used: fresh?.cost_used ?? "0" });
    },
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setTotal(""); toast.success(tc("actions.save")); },
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
        <p className="text-sm text-muted-foreground">
          {t("access.costUsed")}: <span className="font-mono">{quota.cost_used}</span> / <span className="font-mono">{quota.quota_total}</span>
        </p>
      ) : (
        <p className="text-sm text-muted-foreground">{t("access.noQuota")}</p>
      )}
      <form
        className="flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (!isNumeric(total)) return;
          save.mutate();
        }}
      >
        <div className="grid flex-1 gap-1">
          <Label htmlFor={`q-${scope}-${scopeId}`} className="text-xs">{t("access.quotaTotal")}</Label>
          <Input
            id={`q-${scope}-${scopeId}`}
            inputMode="decimal"
            value={total}
            onChange={(e) => setTotal(e.target.value)}
            placeholder={quota?.quota_total ?? "100.00"}
          />
        </div>
        <Button type="submit" disabled={save.isPending || !total.trim() || !isNumeric(total)}>
          {t("access.setQuota")}
        </Button>
        {quota && (
          <Button type="button" variant="ghost" className="text-destructive" onClick={() => setConfirmClear(true)}>
            {t("access.clearQuota")}
          </Button>
        )}
      </form>
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
