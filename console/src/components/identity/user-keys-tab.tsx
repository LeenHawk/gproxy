import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  userKeysQuery, createUserKey, deleteUserKey, type UserView, type UserKeyView,
} from "@/api/identity";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";

export function UserKeysTab({ user }: { user: UserView }) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: keys, isPending } = useQuery(userKeysQuery(user.id));
  const rows = keys ?? [];
  const batch = useBatch("user-keys", ["users", user.id, "keys"]);
  const ids = rows.map((k) => k.id);

  const [generateOpen, setGenerateOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<UserKeyView | undefined>(undefined);

  const generate = useMutation({
    mutationFn: () => createUserKey(user.id, { label: label.trim() || null, enabled: true }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: userKeysQuery(user.id).queryKey });
      setGenerateOpen(false);
      setLabel("");
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteUserKey(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: userKeysQuery(user.id).queryKey });
      toast.success(tc("actions.deleted"));
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const actionsColumn = (k: UserKeyView) => (
    <div className="flex items-center justify-end">
      <Button
        variant="ghost"
        size="icon"
        className="text-destructive"
        aria-label={t("keys.delete")}
        onClick={(e) => { e.stopPropagation(); setDeleteTarget(k); }}
      >
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<UserKeyView>[] = [
    {
      key: "label",
      header: t("keys.label"),
      cell: (k) => <span className="text-sm">{k.label ?? "—"}</span>,
    },
    {
      key: "api_key",
      header: t("keys.key"),
      cell: (k) => <span className="break-all font-mono text-sm">{k.api_key}</span>,
    },
    {
      key: "enabled",
      header: t("keys.enabled"),
      cell: (k) => <Badge variant={k.enabled ? "secondary" : "outline"}>{k.enabled ? "on" : "off"}</Badge>,
    },
    ...(batch.mode ? [] : [{ key: "actions", header: "", cell: actionsColumn, className: "w-16 text-right" } as DataColumn<UserKeyView>]),
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">{t("keys.rotateHint")}</p>
        <div className="flex items-center gap-2">
          {!batch.mode && (
            <Button onClick={() => { setLabel(""); setGenerateOpen(true); }}>
              <Plus className="size-4" aria-hidden />
              {t("keys.add")}
            </Button>
          )}
          <Button variant="outline" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
            {batch.mode ? tc("batch.cancel") : tc("batch.select")}
          </Button>
        </div>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(k) => k.id}
          empty={t("keys.empty")}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(k) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">{k.label ?? "—"}</span>
                <Badge variant={k.enabled ? "secondary" : "outline"}>{k.enabled ? "on" : "off"}</Badge>
              </div>
              <span className="break-all font-mono text-xs text-muted-foreground">{k.api_key}</span>
              {!batch.mode && actionsColumn(k)}
            </div>
          )}
        />
      )}
      {batch.mode && (
        <BatchToolbar
          count={batch.selected.size}
          onEnable={batch.runEnable}
          onDisable={batch.runDisable}
          onDelete={batch.runDelete}
          onCancel={batch.exit}
          pending={batch.pending}
        />
      )}

      {/* Generate key dialog */}
      <EntityDialog open={generateOpen} onOpenChange={setGenerateOpen} title={t("keys.add")}>
        <form
          className="grid gap-4"
          onSubmit={(e) => { e.preventDefault(); generate.mutate(); }}
        >
          <div className="grid gap-2">
            <Label htmlFor="key-label">{t("keys.label")}</Label>
            <Input
              id="key-label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t("keys.label")}
            />
          </div>
          <Button type="submit" disabled={generate.isPending}>{t("keys.add")}</Button>
        </form>
      </EntityDialog>

      {/* Delete confirmation */}
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("keys.delete")}
        description={t("keys.deleteConfirm")}
        confirmLabel={t("keys.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </div>
  );
}
