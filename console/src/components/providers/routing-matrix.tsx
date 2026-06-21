import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, RotateCcw, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { routingRulesQuery, deleteRoutingRule, resetRoutingDefaults, type RoutingRule } from "@/api/rules";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";
import { RoutingCellEditor, type CellInitial } from "./routing-cell-editor";

function behaviorBadge(impl: string, destKind: string | null, t: (k: string) => string) {
  const label = t(`implementation.${impl}`);
  if (impl === "transform_to") {
    return (
      <Badge variant="outline" className="font-mono text-xs text-blue-700 border-blue-300 dark:text-blue-400 dark:border-blue-700">
        {label}{destKind ? ` → ${t(`protocolKind.${destKind}`)}` : ""}
      </Badge>
    );
  }
  if (impl === "passthrough") {
    return <Badge variant="secondary" className="font-mono text-xs bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400">{label}</Badge>;
  }
  if (impl === "local") {
    return <Badge variant="outline" className="font-mono text-xs text-amber-700 border-amber-300 dark:text-amber-400 dark:border-amber-700">{label}</Badge>;
  }
  return <Badge variant="destructive" className="font-mono text-xs">{label}</Badge>;
}

export function RoutingMatrix({ providerId }: { providerId: number }) {
  const { t } = useTranslation("rules");
  const queryClient = useQueryClient();
  const { data: rows, isPending } = useQuery(routingRulesQuery(providerId));

  const [editorOpen, setEditorOpen] = useState(false);
  const [target, setTarget] = useState<{ mode: "add" | "edit"; initial: CellInitial } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<RoutingRule | undefined>(undefined);
  const [resetConfirm, setResetConfirm] = useState(false);

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["providers", providerId, "routing-rules"] });

  const removal = useMutation({
    mutationFn: (id: number) => deleteRoutingRule(id),
    onSuccess: () => { void invalidate(); setDeleteTarget(undefined); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setDeleteTarget(undefined); },
  });
  const reset = useMutation({
    mutationFn: () => resetRoutingDefaults(providerId),
    onSuccess: () => { void invalidate(); setResetConfirm(false); toast.success(t("routing.resetDone")); },
    onError: (e) => { toast.error(e instanceof ApiError ? e.message : String(e)); setResetConfirm(false); },
  });

  const openEdit = (row: RoutingRule) => {
    setTarget({ mode: "edit", initial: { operation: row.operation, kind: row.kind, implementation: row.implementation, destKind: row.dest_kind, ruleId: row.id, sortOrder: row.sort_order } });
    setEditorOpen(true);
  };
  const openAdd = () => {
    setTarget({ mode: "add", initial: { operation: "generate_content", kind: "open_ai_chat_completions", implementation: "passthrough", destKind: "claude_messages" } });
    setEditorOpen(true);
  };

  const list = rows ?? [];

  return (
    <section className="grid gap-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold">{t("routing.title")}</h3>
          <p className="text-xs text-muted-foreground mt-1">{t("routing.caption")}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button variant="outline" onClick={() => setResetConfirm(true)} disabled={reset.isPending}>
            <RotateCcw className="size-4" aria-hidden />
            {t("routing.resetAll")}
          </Button>
          <Button onClick={openAdd}>
            <Plus className="size-4" aria-hidden />
            {t("routing.add")}
          </Button>
        </div>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          {Array.from({ length: 4 }).map((_, i) => <Skeleton key={i} className="h-9" />)}
        </div>
      ) : list.length === 0 ? (
        <div className="grid place-items-center gap-3 rounded-md border border-dashed py-10 text-center">
          <p className="text-sm text-muted-foreground">{t("routing.empty")}</p>
          <Button onClick={() => reset.mutate()} disabled={reset.isPending}>
            <RotateCcw className="size-4" aria-hidden />
            {t("routing.initialize")}
          </Button>
        </div>
      ) : (
        <div className="overflow-x-auto rounded-md border">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50">
                <th className="px-3 py-2 text-left font-medium text-muted-foreground">{t("routing.columns.operation")}</th>
                <th className="px-3 py-2 text-left font-medium text-muted-foreground">{t("routing.columns.kind")}</th>
                <th className="px-3 py-2 text-left font-medium text-muted-foreground">{t("routing.columns.behavior")}</th>
                <th className="w-20 px-3 py-2" />
              </tr>
            </thead>
            <tbody>
              {list.map((row, i) => (
                <tr
                  key={row.id}
                  onClick={() => openEdit(row)}
                  className={cn("cursor-pointer hover:bg-accent/50", i % 2 === 0 ? "bg-background" : "bg-muted/20")}
                >
                  <td className="px-3 py-2">{t(`operation.${row.operation}`)}</td>
                  <td className="px-3 py-2">{t(`protocolKind.${row.kind}`)}</td>
                  <td className="px-3 py-2">{behaviorBadge(row.implementation, row.dest_kind, t)}</td>
                  <td className="px-3 py-2">
                    <div className="flex items-center justify-end gap-1" onClick={(e) => e.stopPropagation()}>
                      <Button variant="ghost" size="icon" aria-label={t("routing.editTitle")} onClick={() => openEdit(row)}>
                        <Pencil className="size-4" aria-hidden />
                      </Button>
                      <Button variant="ghost" size="icon" className="text-destructive" aria-label={t("routing.delete")} onClick={() => setDeleteTarget(row)}>
                        <Trash2 className="size-4" aria-hidden />
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <EntityDialog
        open={editorOpen}
        onOpenChange={(o) => { setEditorOpen(o); if (!o) setTarget(null); }}
        title={target?.mode === "add" ? t("routing.addTitle") : t("routing.editTitle")}
      >
        {target && (
          <RoutingCellEditor
            key={`${target.mode}:${target.initial.operation}:${target.initial.kind}`}
            providerId={providerId}
            mode={target.mode}
            initial={target.initial}
            onSaved={() => { setEditorOpen(false); setTarget(null); }}
          />
        )}
      </EntityDialog>

      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("routing.delete")}
        description={t("routing.deleteConfirm")}
        confirmLabel={t("routing.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
      <ConfirmDangerous
        open={resetConfirm}
        onOpenChange={setResetConfirm}
        title={t("routing.resetAll")}
        description={t("routing.resetConfirm")}
        confirmLabel={t("routing.resetAll")}
        onConfirm={() => reset.mutate()}
        pending={reset.isPending}
      />
    </section>
  );
}
