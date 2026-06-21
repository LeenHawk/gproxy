import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteRouteMember, routeMembersQuery, type Route, type RouteMember } from "@/api/routes";
import { providersQuery } from "@/api/providers";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { MemberForm } from "@/components/routes/member-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export function MembersTab({ route }: { route: Route }) {
  const { t } = useTranslation("routes");
  const queryClient = useQueryClient();
  const { data: members, isPending } = useQuery(routeMembersQuery(route.id));
  const { data: providers } = useQuery(providersQuery);
  const providerName = (id: number) => {
    const p = providers?.find((x) => x.id === id);
    return p ? (p.label ?? p.name) : `#${id}`;
  };

  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<RouteMember | undefined>(undefined);
  const [deleteTarget, setDeleteTarget] = useState<RouteMember | undefined>(undefined);

  const removal = useMutation({
    mutationFn: (id: number) => deleteRouteMember(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["routes", route.id, "members"] });
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const openCreate = () => {
    setEditTarget(undefined);
    setFormOpen(true);
  };
  const actions = (m: RouteMember) => (
    <div className="flex items-center justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("members.edit")}
        onClick={(e) => {
          e.stopPropagation();
          setEditTarget(m);
          setFormOpen(true);
        }}
      >
        <Pencil className="size-4" aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="text-destructive"
        aria-label={t("delete.member")}
        onClick={(e) => {
          e.stopPropagation();
          setDeleteTarget(m);
        }}
      >
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<RouteMember>[] = [
    {
      key: "provider",
      header: t("members.provider"),
      cell: (m) => <span className="font-medium">{providerName(m.provider_id)}</span>,
    },
    {
      key: "model",
      header: t("members.model"),
      cell: (m) => <span className="font-mono text-xs">{m.upstream_model_id}</span>,
    },
    { key: "tier", header: t("members.tier"), cell: (m) => m.tier },
    { key: "weight", header: t("members.weight"), cell: (m) => m.weight },
    {
      key: "enabled",
      header: t("fields.enabled"),
      cell: (m) => (
        <Badge variant={m.enabled ? "secondary" : "outline"}>{m.enabled ? "on" : "off"}</Badge>
      ),
    },
    { key: "actions", header: "", cell: actions, className: "w-20 text-right" },
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end">
        <Button onClick={openCreate}>
          <Plus className="size-4" aria-hidden />
          {t("members.add")}
        </Button>
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={members ?? []}
          rowKey={(m) => m.id}
          empty={t("members.empty")}
          renderCard={(m) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="font-medium">{providerName(m.provider_id)}</span>
                <Badge variant={m.enabled ? "secondary" : "outline"}>
                  {m.enabled ? "on" : "off"}
                </Badge>
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <span className="font-mono">{m.upstream_model_id}</span>
                <span>tier {m.tier}</span>
                <span>w{m.weight}</span>
              </div>
              {actions(m)}
            </div>
          )}
        />
      )}
      <EntityDialog
        open={formOpen}
        onOpenChange={setFormOpen}
        title={editTarget ? t("members.edit") : t("members.add")}
      >
        <MemberForm
          key={editTarget?.id ?? "new"}
          routeId={route.id}
          member={editTarget}
          onSaved={() => setFormOpen(false)}
        />
      </EntityDialog>
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => {
          if (!o) setDeleteTarget(undefined);
        }}
        title={t("delete.member")}
        description={t("delete.memberConfirm")}
        confirmLabel={t("delete.member")}
        onConfirm={() => {
          if (deleteTarget) removal.mutate(deleteTarget.id);
        }}
        pending={removal.isPending}
      />
    </div>
  );
}
