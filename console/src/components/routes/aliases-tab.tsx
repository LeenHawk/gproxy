import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { aliasesQuery, deleteAlias, upsertAlias, type Alias } from "@/api/aliases";
import type { Route } from "@/api/routes";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";

export function AliasesTab({ route }: { route: Route }) {
  const { t } = useTranslation("routes");
  const queryClient = useQueryClient();
  const { data: all, isPending } = useQuery(aliasesQuery);
  const aliases = (all ?? []).filter((a) => a.route_id === route.id);

  const [createOpen, setCreateOpen] = useState(false);
  const [alias, setAlias] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Alias | undefined>(undefined);

  const creation = useMutation({
    mutationFn: () => {
      if (!alias.trim()) throw new ApiError(0, "bad_request", t("form.required"));
      return upsertAlias({ alias: alias.trim(), route_id: route.id });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["aliases"] });
      setAlias("");
      setCreateOpen(false);
    },
    onError: (error) => {
      setFormError(
        error instanceof ApiError && error.status === 409
          ? t("aliases.duplicate")
          : error instanceof ApiError
            ? error.message
            : String(error),
      );
    },
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteAlias(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["aliases"] });
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const deleteButton = (a: Alias) => (
    <Button
      variant="ghost"
      size="icon"
      className="text-destructive"
      aria-label={t("delete.alias")}
      onClick={(e) => {
        e.stopPropagation();
        setDeleteTarget(a);
      }}
    >
      <Trash2 className="size-4" aria-hidden />
    </Button>
  );

  const columns: DataColumn<Alias>[] = [
    { key: "alias", header: t("aliases.alias"), cell: (a) => <span className="font-mono">{a.alias}</span> },
    {
      key: "actions",
      header: "",
      cell: (a) => <div className="flex justify-end">{deleteButton(a)}</div>,
      className: "w-16 text-right",
    },
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end">
        <Button
          onClick={() => {
            setFormError(null);
            setAlias("");
            setCreateOpen(true);
          }}
        >
          <Plus className="size-4" aria-hidden />
          {t("aliases.add")}
        </Button>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={aliases}
          rowKey={(a) => a.id}
          empty={t("aliases.empty")}
          renderCard={(a) => (
            <div className="flex items-center justify-between">
              <span className="font-mono">{a.alias}</span>
              {deleteButton(a)}
            </div>
          )}
        />
      )}

      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("aliases.add")}>
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            setFormError(null);
            creation.mutate();
          }}
        >
          <div className="grid gap-2">
            <Label htmlFor="a-alias">{t("aliases.alias")}</Label>
            <Input
              id="a-alias"
              value={alias}
              onChange={(e) => setAlias(e.target.value)}
              placeholder="gpt-4o"
              autoFocus
            />
            <p className="text-xs text-muted-foreground">{t("aliases.aliasHint")}</p>
          </div>
          {formError && <p className="text-sm text-destructive">{formError}</p>}
          <Button type="submit" disabled={creation.isPending}>
            {t("aliases.add")}
          </Button>
        </form>
      </EntityDialog>

      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => {
          if (!o) setDeleteTarget(undefined);
        }}
        title={t("delete.alias")}
        description={t("delete.aliasConfirm", { name: deleteTarget?.alias ?? "" })}
        confirmLabel={t("delete.alias")}
        onConfirm={() => {
          if (deleteTarget) removal.mutate(deleteTarget.id);
        }}
        pending={removal.isPending}
      />
    </div>
  );
}
