import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { orgsQuery, usersQuery, type UserView } from "@/api/identity";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { UserForm } from "@/components/identity/user-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useBatch } from "@/hooks/use-batch";
import { BatchToolbar } from "@/components/batch-toolbar";

export const Route = createFileRoute("/_app/users/")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(orgsQuery);
    return context.queryClient.ensureQueryData(usersQuery);
  },
  component: UsersPage,
});

function UsersPage() {
  const { t } = useTranslation("identity");
  const navigate = useNavigate();
  const { data: users, isPending } = useQuery(usersQuery);
  const { data: orgs } = useQuery(orgsQuery);
  const [createOpen, setCreateOpen] = useState(false);

  const rows = users ?? [];
  const batch = useBatch("users", ["users"]);
  const ids = rows.map((u) => u.id);

  const orgMap = new Map((orgs ?? []).map((o) => [o.id, o.name]));

  const columns: DataColumn<UserView>[] = [
    {
      key: "name",
      header: t("users.name"),
      cell: (u) => <span className="font-medium">{u.name}</span>,
    },
    {
      key: "org",
      header: t("users.org"),
      cell: (u) => (
        <span className="text-sm text-muted-foreground">
          {orgMap.get(u.org_id) ?? `#${u.org_id}`}
        </span>
      ),
    },
    {
      key: "is_admin",
      header: t("users.isAdmin"),
      cell: (u) =>
        u.is_admin ? (
          <Badge variant="secondary">{t("users.isAdmin")}</Badge>
        ) : null,
    },
    {
      key: "has_password",
      header: t("users.password"),
      cell: (u) =>
        u.has_password ? (
          <Badge variant="secondary">{t("users.hasPassword")}</Badge>
        ) : (
          <span className="text-xs text-muted-foreground">{t("users.noPassword")}</span>
        ),
    },
    {
      key: "enabled",
      header: t("users.enabled"),
      cell: (u) => (
        <Badge variant={u.enabled ? "secondary" : "outline"}>{u.enabled ? "on" : "off"}</Badge>
      ),
    },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("users.title")}</h1>
        <div className="flex items-center gap-2">
          <Button variant="outline" onClick={() => batch.setMode(!batch.mode)}>
            {batch.mode ? t("batch.cancel", { ns: "common" }) : t("batch.select", { ns: "common" })}
          </Button>
          <Button onClick={() => setCreateOpen(true)}>
            <Plus className="size-4" aria-hidden />
            <span className="hidden sm:inline">{t("users.new")}</span>
          </Button>
        </div>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(u) => u.id}
          empty={t("users.empty")}
          onRowClick={batch.mode ? undefined : (u) => void navigate({ to: "/users/$userId", params: { userId: String(u.id) } })}
          selection={batch.mode ? {
            selectedIds: batch.selected,
            onToggle: batch.toggle,
            onToggleAll: () => batch.toggleAllFor(ids),
            allSelected: batch.allSelectedFor(ids),
            indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
          } : undefined}
          renderCard={(u) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium">{u.name}</span>
                <div className="flex items-center gap-1">
                  {u.is_admin && <Badge variant="secondary">{t("users.isAdmin")}</Badge>}
                  <Badge variant={u.enabled ? "secondary" : "outline"}>{u.enabled ? "on" : "off"}</Badge>
                </div>
              </div>
              <span className="text-xs text-muted-foreground">
                {orgMap.get(u.org_id) ?? `#${u.org_id}`}
              </span>
              {u.has_password ? (
                <Badge variant="secondary" className="w-fit">{t("users.hasPassword")}</Badge>
              ) : (
                <span className="text-xs text-muted-foreground">{t("users.noPassword")}</span>
              )}
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

      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("users.new")}>
        <UserForm
          onSaved={(saved) => {
            setCreateOpen(false);
            void navigate({ to: "/users/$userId", params: { userId: String(saved.id) } });
          }}
        />
      </EntityDialog>
    </div>
  );
}
