import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { orgsQuery, type Org } from "@/api/identity";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { OrgForm } from "@/components/identity/org-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/orgs/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(orgsQuery),
  component: OrgsPage,
});

function EnabledBadge({ enabled }: { enabled: boolean }) {
  return <Badge variant={enabled ? "secondary" : "outline"}>{enabled ? "on" : "off"}</Badge>;
}

function OrgsPage() {
  const { t } = useTranslation("identity");
  const navigate = useNavigate();
  const { data: orgs, isPending } = useQuery(orgsQuery);
  const [createOpen, setCreateOpen] = useState(false);

  const columns: DataColumn<Org>[] = [
    {
      key: "name",
      header: t("orgs.name"),
      cell: (org) => <span className="font-medium">{org.name}</span>,
    },
    {
      key: "description",
      header: t("orgs.description"),
      cell: (org) => (
        <span className="text-sm text-muted-foreground">{org.description ?? "—"}</span>
      ),
    },
    {
      key: "enabled",
      header: t("orgs.enabled"),
      cell: (org) => <EnabledBadge enabled={org.enabled} />,
    },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("orgs.title")}</h1>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus className="size-4" aria-hidden />
          <span className="hidden sm:inline">{t("orgs.new")}</span>
        </Button>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={orgs ?? []}
          rowKey={(org) => org.id}
          empty={t("orgs.empty")}
          onRowClick={(org) => void navigate({ to: "/orgs/$orgId", params: { orgId: String(org.id) } })}
          renderCard={(org) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between">
                <span className="font-medium">{org.name}</span>
                <EnabledBadge enabled={org.enabled} />
              </div>
              {org.description && (
                <p className="text-xs text-muted-foreground">{org.description}</p>
              )}
            </div>
          )}
        />
      )}

      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("orgs.new")}>
        <OrgForm
          onSaved={(saved) => {
            setCreateOpen(false);
            void navigate({ to: "/orgs/$orgId", params: { orgId: String(saved.id) } });
          }}
        />
      </EntityDialog>
    </div>
  );
}
