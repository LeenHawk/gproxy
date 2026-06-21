import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { routesQuery, type Route as RouteRecord } from "@/api/routes";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { RouteForm } from "@/components/routes/route-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/routes/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(routesQuery),
  component: RoutesPage,
});

function RoutesPage() {
  const { t } = useTranslation("routes");
  const navigate = useNavigate();
  const { data: routes, isPending } = useQuery(routesQuery);
  const [createOpen, setCreateOpen] = useState(false);

  const columns: DataColumn<RouteRecord>[] = [
    { key: "name", header: t("fields.name"), cell: (r) => <span className="font-medium">{r.name}</span> },
    { key: "strategy", header: t("fields.strategy"), cell: (r) => t(`strategy.${r.strategy}`, { defaultValue: r.strategy }) },
    { key: "description", header: t("fields.description"), cell: (r) => (
      <span className="text-sm text-muted-foreground">{r.description ?? "—"}</span>
    ) },
    { key: "enabled", header: t("fields.enabled"), cell: (r) => (
      <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? "on" : "off"}</Badge>
    ) },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">{t("title")}</h1>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus className="size-4" aria-hidden />
          <span className="hidden sm:inline">{t("new")}</span>
        </Button>
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true"><Skeleton className="h-10" /><Skeleton className="h-10" /></div>
      ) : (
        <DataTable
          columns={columns}
          rows={routes ?? []}
          rowKey={(r) => r.id}
          empty={t("empty")}
          onRowClick={(r) => void navigate({ to: "/routes/$routeId", params: { routeId: String(r.id) } })}
          renderCard={(r) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between">
                <span className="font-medium">{r.name}</span>
                <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? "on" : "off"}</Badge>
              </div>
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>{t(`strategy.${r.strategy}`, { defaultValue: r.strategy })}</span>
                {r.description ? <><span>·</span><span className="truncate">{r.description}</span></> : null}
              </div>
            </div>
          )}
        />
      )}
      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("new")}>
        <RouteForm onSaved={(saved) => {
          setCreateOpen(false);
          void navigate({ to: "/routes/$routeId", params: { routeId: String(saved.id) } });
        }} />
      </EntityDialog>
    </div>
  );
}
