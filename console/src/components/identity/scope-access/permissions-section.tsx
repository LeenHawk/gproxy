import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  type Scope, permissionsQuery, upsertPermission, deletePermission, type RoutePermission,
} from "@/api/authz";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function PermissionsSection({ scope, scopeId }: { scope: Scope; scopeId: number }) {
  const { t } = useTranslation("identity");
  const qc = useQueryClient();
  const key = ["route-permissions", scope, scopeId];
  const { data } = useQuery(permissionsQuery(scope, scopeId));
  const [pattern, setPattern] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<RoutePermission | undefined>(undefined);

  const add = useMutation({
    mutationFn: () => upsertPermission({ scope, scope_id: scopeId, route_pattern: pattern.trim() }),
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setPattern(""); },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  });

  const removal = useMutation({
    mutationFn: (id: number) => deletePermission(id),
    // micro-edit deletes: row vanishing is the feedback — no success toast (kept intentionally; F4+ copies this)
    onSuccess: () => { void qc.invalidateQueries({ queryKey: key }); setDeleteTarget(undefined); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setDeleteTarget(undefined); },
  });

  return (
    <section className="grid gap-2">
      <div>
        <h3 className="text-sm font-medium">{t("access.permissions")}</h3>
        <p className="text-xs text-muted-foreground">{t("access.permissionsHint")}</p>
      </div>
      {(data ?? []).length === 0 && <p className="text-sm text-muted-foreground">{t("access.noPermissions")}</p>}
      <ul className="grid gap-1">
        {(data ?? []).map((p: RoutePermission) => (
          <li key={p.id} className="flex items-center justify-between rounded-md border px-3 py-1.5">
            <span className="font-mono text-sm">{p.route_pattern}</span>
            <Button
              variant="ghost"
              size="icon"
              className="text-destructive"
              aria-label={t("access.deletePermission")}
              onClick={() => setDeleteTarget(p)}
            >
              <Trash2 className="size-4" aria-hidden />
            </Button>
          </li>
        ))}
      </ul>
      <form className="flex items-end gap-2" onSubmit={(e) => { e.preventDefault(); if (pattern.trim()) add.mutate(); }}>
        <div className="grid flex-1 gap-1">
          <Label htmlFor={`perm-${scope}-${scopeId}`} className="sr-only">{t("access.routePattern")}</Label>
          <Input id={`perm-${scope}-${scopeId}`} value={pattern} onChange={(e) => setPattern(e.target.value)} placeholder="gpt-4o / claude-* / *" />
        </div>
        <Button type="submit" disabled={add.isPending || !pattern.trim()}>
          <Plus className="size-4" aria-hidden />
          {t("access.addPermission")}
        </Button>
      </form>
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("access.deletePermission")}
        description={t("access.deletePermissionConfirm")}
        confirmLabel={t("access.deletePermission")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </section>
  );
}
