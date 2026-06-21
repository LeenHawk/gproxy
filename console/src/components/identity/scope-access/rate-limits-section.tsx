import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type Scope, rateLimitsQuery, upsertRateLimit, deleteRateLimit, type RateLimit,
} from "@/api/authz";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function RateLimitsSection({ scope, scopeId }: { scope: Scope; scopeId: number }) {
  const { t } = useTranslation("identity");
  const qc = useQueryClient();
  const key = ["rate-limits", scope, scopeId];
  const { data } = useQuery(rateLimitsQuery(scope, scopeId));
  const [pattern, setPattern] = useState("");
  const [rpm, setRpm] = useState(""); const [rpd, setRpd] = useState(""); const [tot, setTot] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<RateLimit | undefined>(undefined);

  const intOrNull = (v: string) => { const n = Number(v); return v.trim() !== "" && Number.isInteger(n) && n >= 0 ? n : null; };
  const noLimits = rpm.trim() === "" && rpd.trim() === "" && tot.trim() === "";

  const add = useMutation({
    mutationFn: () => upsertRateLimit({ scope, scope_id: scopeId, route_pattern: pattern.trim() || "*", rpm: intOrNull(rpm), rpd: intOrNull(rpd), total_tokens: intOrNull(tot) }),
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setPattern(""); setRpm(""); setRpd(""); setTot(""); },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteRateLimit(id),
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setDeleteTarget(undefined); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setDeleteTarget(undefined); },
  });

  return (
    <section className="grid gap-2">
      <div>
        <h3 className="text-sm font-medium">{t("access.rateLimits")}</h3>
        <p className="text-xs text-muted-foreground">{t("access.rateLimitsHint")}</p>
      </div>
      {(data ?? []).length === 0 && <p className="text-sm text-muted-foreground">{t("access.noRateLimits")}</p>}
      <ul className="grid gap-1">
        {(data ?? []).map((r: RateLimit) => (
          <li key={r.id} className="flex items-center justify-between rounded-md border px-3 py-1.5 text-sm">
            <span className="font-mono">{r.route_pattern}</span>
            <span className="flex items-center gap-3 text-muted-foreground">
              <span>{t("access.rpm")} {r.rpm ?? "—"}</span>
              <span>{t("access.rpd")} {r.rpd ?? "—"}</span>
              <span>{t("access.totalTokens")} {r.total_tokens ?? "—"}</span>
              <Button
                variant="ghost"
                size="icon"
                className="text-destructive"
                aria-label={t("access.deleteRateLimit")}
                onClick={() => setDeleteTarget(r)}
              >
                <Trash2 className="size-4" aria-hidden />
              </Button>
            </span>
          </li>
        ))}
      </ul>
      <form
        className="grid gap-2 sm:grid-cols-[1fr_auto_auto_auto_auto] sm:items-end"
        onSubmit={(e) => { e.preventDefault(); if (!noLimits) add.mutate(); }}
      >
        <div className="grid gap-1">
          <Label htmlFor={`rl-pat-${scope}-${scopeId}`} className="text-xs">{t("access.routePattern")}</Label>
          <Input id={`rl-pat-${scope}-${scopeId}`} value={pattern} onChange={(e) => setPattern(e.target.value)} placeholder="*" />
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`rl-rpm-${scope}-${scopeId}`} className="text-xs">{t("access.rpm")}</Label>
          <Input id={`rl-rpm-${scope}-${scopeId}`} inputMode="numeric" value={rpm} onChange={(e) => setRpm(e.target.value)} className="w-20" />
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`rl-rpd-${scope}-${scopeId}`} className="text-xs">{t("access.rpd")}</Label>
          <Input id={`rl-rpd-${scope}-${scopeId}`} inputMode="numeric" value={rpd} onChange={(e) => setRpd(e.target.value)} className="w-20" />
        </div>
        <div className="grid gap-1">
          <Label htmlFor={`rl-tot-${scope}-${scopeId}`} className="text-xs">{t("access.totalTokens")}</Label>
          <Input id={`rl-tot-${scope}-${scopeId}`} inputMode="numeric" value={tot} onChange={(e) => setTot(e.target.value)} className="w-24" />
        </div>
        <Button type="submit" disabled={add.isPending || noLimits}>
          <Plus className="size-4" aria-hidden />
          {t("access.addRateLimit")}
        </Button>
      </form>
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("access.deleteRateLimit")}
        description={t("access.deleteRateLimitConfirm")}
        confirmLabel={t("access.deleteRateLimit")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </section>
  );
}
