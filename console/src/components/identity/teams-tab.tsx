import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Shield, Trash2, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { teamsQuery, deleteTeam, type Org, type Team } from "@/api/identity";
import { ApiError } from "@/api/http";
import { BatchToolbar } from "@/components/batch-toolbar";
import { ScopeAccessEditor } from "@/components/identity/scope-access-editor";
import { TeamForm } from "@/components/identity/team-form";
import { useTeamToggle } from "@/components/identity/use-team-toggle";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useBatch } from "@/hooks/use-batch";

export function TeamsTab({ org }: { org: Org }) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: teams, isPending } = useQuery(teamsQuery(org.id));
  const rows = teams ?? [];
  const batch = useBatch("teams", ["orgs", org.id, "teams"]);
  const toggle = useTeamToggle(org.id);
  const ids = rows.map((team) => team.id);

  const [formOpen, setFormOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<Team | undefined>(undefined);
  const [deleteTarget, setDeleteTarget] = useState<Team | undefined>(undefined);
  const [accessTarget, setAccessTarget] = useState<Team | undefined>(undefined);

  const removal = useMutation({
    mutationFn: (id: number) => deleteTeam(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["orgs", org.id, "teams"] });
      toast.success(tc("actions.deleted"));
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const openCreate = () => { setEditTarget(undefined); setFormOpen(true); };
  const openEdit = (team: Team) => { setEditTarget(team); setFormOpen(true); };
  const enabledControl = (team: Team) => batch.mode ? (
    <Badge variant={team.enabled ? "secondary" : "outline"}>
      {team.enabled ? t("teams.status.enabled") : t("teams.status.disabled")}
    </Badge>
  ) : (
    <Switch
      size="sm"
      checked={team.enabled}
      disabled={toggle.isPending}
      aria-label={team.enabled ? t("teams.disable") : t("teams.enable")}
      onCheckedChange={(enabled) => toggle.mutate({ team, enabled })}
    />
  );

  const actionsColumn = (team: Team) => (
    <div className="flex items-center justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("teams.access")}
        onClick={(e) => { e.stopPropagation(); setAccessTarget(team); }}
      >
        <Shield className="size-4" aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label={t("teams.edit")}
        onClick={(e) => { e.stopPropagation(); openEdit(team); }}
      >
        <Pencil className="size-4" aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="text-destructive"
        aria-label={t("teams.delete")}
        onClick={(e) => { e.stopPropagation(); setDeleteTarget(team); }}
      >
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<Team>[] = [
    {
      key: "name",
      header: t("teams.name"),
      cell: (team) => <span className="font-medium">{team.name}</span>,
    },
    {
      key: "enabled",
      header: t("teams.enabled"),
      cell: enabledControl,
    },
    ...(batch.mode ? [] : [{ key: "actions", header: "", cell: actionsColumn, className: "w-32 text-right" } as DataColumn<Team>]),
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-end gap-2">
        {!batch.mode && (
          <Button onClick={openCreate}>
            <Plus className="size-4" aria-hidden />
            {t("teams.add")}
          </Button>
        )}
        <Button variant="outline" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
          {batch.mode ? tc("batch.cancel") : tc("batch.select")}
        </Button>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(team) => team.id}
          empty={t("teams.empty")}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(team) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="font-medium">{team.name}</span>
                {enabledControl(team)}
              </div>
              {!batch.mode && actionsColumn(team)}
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

      <EntityDialog
        open={formOpen}
        onOpenChange={setFormOpen}
        title={editTarget ? t("teams.edit") : t("teams.add")}
      >
        <TeamForm
          key={editTarget?.id ?? "new"}
          orgId={org.id}
          team={editTarget}
          onSaved={() => setFormOpen(false)}
        />
      </EntityDialog>

      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("teams.delete")}
        description={t("teams.deleteConfirm", { name: deleteTarget?.name ?? "" })}
        confirmLabel={t("teams.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />

      <EntityDialog
        open={accessTarget !== undefined}
        onOpenChange={(o) => { if (!o) setAccessTarget(undefined); }}
        title={`${t("teams.access")} — ${accessTarget?.name ?? ""}`}
        wide
      >
        {accessTarget && (
          <ScopeAccessEditor scope="team" scopeId={accessTarget.id} />
        )}
      </EntityDialog>
    </div>
  );
}
