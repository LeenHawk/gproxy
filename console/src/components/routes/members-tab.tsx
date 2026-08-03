import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { deleteRouteMember, routeMembersQuery, type Route, type RouteMember } from "@/api/routes";
import { providersQuery } from "@/api/providers";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { MemberForm } from "@/components/routes/member-form";
import { MemberCard, MemberEnabledField, MemberIntegerField } from "@/components/routes/member-inline-fields";
import { useRouteMemberUpdate, type MemberChanges } from "@/components/routes/use-route-member-update";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";

export function MembersTab({ route }: { route: Route }) {
  const { t } = useTranslation("routes");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: members, isPending } = useQuery(routeMembersQuery(route.id));
  const { data: providers } = useQuery(providersQuery);
  const rows = members ?? [];
  const batch = useBatch("route-members", ["routes", route.id, "members"]);
  const update = useRouteMemberUpdate(route.id);
  const ids = rows.map((m) => m.id);
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
  const inlineProps = (member: RouteMember) => ({
    member,
    selecting: batch.mode,
    pending: update.isPending,
    onChange: (changes: MemberChanges) => update.mutate({ member, changes }),
  });
  const actionsColumn = (m: RouteMember) => (
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
    {
      key: "tier",
      header: t("members.tier"),
      cell: (m) => <MemberIntegerField {...inlineProps(m)} field="tier" />,
    },
    {
      key: "weight",
      header: t("members.weight"),
      cell: (m) => <MemberIntegerField {...inlineProps(m)} field="weight" />,
    },
    {
      key: "enabled",
      header: t("fields.enabled"),
      cell: (m) => <MemberEnabledField {...inlineProps(m)} />,
    },
    ...(batch.mode ? [] : [{ key: "actions", header: "", cell: actionsColumn, className: "w-20 text-right" } as DataColumn<RouteMember>]),
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end gap-2">
        {!batch.mode && (
          <Button onClick={openCreate}>
            <Plus className="size-4" aria-hidden />
            {t("members.add")}
          </Button>
        )}
        <Button variant="outline" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
          {batch.mode ? tc("batch.cancel") : tc("batch.select")}
        </Button>
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(m) => m.id}
          empty={t("members.empty")}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(m) => <MemberCard {...inlineProps(m)} providerName={providerName(m.provider_id)} actions={actionsColumn(m)} />}
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
