import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { myKeysQuery, updateMyKey, deleteMyKey } from "@/api/portal";
import type { UserKeyView } from "@/api/identity";
import { ApiError } from "@/api/http";
import { MyKeysCreate } from "@/components/portal/my-keys-create";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";

export const Route = createFileRoute("/_portal/account/keys/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(myKeysQuery),
  component: MyKeysPage,
});

function MyKeysPage() {
  const { t } = useTranslation("portal");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: keys, isPending } = useQuery(myKeysQuery);

  const [createOpen, setCreateOpen] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<UserKeyView | undefined>(undefined);

  const toggle = useMutation({
    mutationFn: ({ id, label, enabled }: { id: number; label: string | null; enabled: boolean }) =>
      updateMyKey(id, { label: label ?? undefined, enabled }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: myKeysQuery.queryKey });
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteMyKey(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: myKeysQuery.queryKey });
      toast.success(tc("actions.deleted"));
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const columns: DataColumn<UserKeyView>[] = [
    {
      key: "label",
      header: t("keys.label"),
      cell: (k) => <span className="text-sm">{k.label ?? "—"}</span>,
    },
    {
      key: "key_prefix",
      header: t("keys.prefix"),
      cell: (k) => <span className="font-mono text-sm">{k.key_prefix}</span>,
    },
    {
      key: "enabled",
      header: t("keys.enabled"),
      cell: (k) => <Badge variant={k.enabled ? "secondary" : "outline"}>{k.enabled ? t("keys.enabled") : t("keys.disabled")}</Badge>,
    },
    {
      key: "toggle",
      header: "",
      cell: (k) => (
        <Switch
          size="sm"
          checked={k.enabled}
          aria-label={k.enabled ? t("keys.disable") : t("keys.enable")}
          disabled={toggle.isPending}
          onCheckedChange={(checked) =>
            toggle.mutate({ id: k.id, label: k.label, enabled: checked })
          }
        />
      ),
      className: "w-12",
    },
    {
      key: "actions",
      header: "",
      cell: (k) => (
        <Button
          variant="ghost"
          size="sm"
          className="text-destructive"
          aria-label={t("keys.delete")}
          onClick={(e) => { e.stopPropagation(); setDeleteTarget(k); }}
        >
          {tc("actions.delete")}
        </Button>
      ),
      className: "w-20 text-right",
    },
  ];

  return (
    <div className="grid gap-4 p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold">{t("pages.keys.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("pages.keys.subtitle")}</p>
        </div>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus className="size-4" aria-hidden />
          {t("keys.add")}
        </Button>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={keys ?? []}
          rowKey={(k) => k.id}
          empty={t("keys.empty")}
          renderCard={(k) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">{k.label ?? "—"}</span>
                <Badge variant={k.enabled ? "secondary" : "outline"}>
                  {k.enabled ? t("keys.enabled") : t("keys.disabled")}
                </Badge>
              </div>
              <span className="font-mono text-xs text-muted-foreground">{k.key_prefix}</span>
              <div className="flex items-center justify-between">
                <Switch
                  size="sm"
                  checked={k.enabled}
                  aria-label={k.enabled ? t("keys.disable") : t("keys.enable")}
                  disabled={toggle.isPending}
                  onCheckedChange={(checked) =>
                    toggle.mutate({ id: k.id, label: k.label, enabled: checked })
                  }
                />
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-destructive"
                  aria-label={t("keys.delete")}
                  onClick={() => setDeleteTarget(k)}
                >
                  {tc("actions.delete")}
                </Button>
              </div>
            </div>
          )}
        />
      )}

      <MyKeysCreate open={createOpen} onOpenChange={setCreateOpen} />

      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("keys.delete")}
        description={t("keys.deleteConfirm")}
        confirmLabel={tc("actions.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </div>
  );
}
