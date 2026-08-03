import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Network, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { aliasesQuery } from "@/api/aliases";
import { ApiError } from "@/api/http";
import { routesQuery, upsertRoute, type Route as RouteRecord } from "@/api/routes";
import { WorkspaceBatchBar } from "@/components/workspace/workspace-batch-bar";
import { WorkspaceLayout } from "@/components/workspace/workspace-layout";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useBatch } from "@/hooks/use-batch";

export const Route = createFileRoute("/_app/routes")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(aliasesQuery);
    return context.queryClient.ensureQueryData(routesQuery);
  },
  component: RoutesWorkspace,
});

function RoutesWorkspace() {
  const { t } = useTranslation("routes");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const selectedId = /(?:^|\/)routes\/([^/]+)/.exec(pathname)?.[1] ?? null;
  const { data: routes } = useQuery(routesQuery);
  const { data: aliases } = useQuery(aliasesQuery);
  const rows = routes ?? [];
  const batch = useBatch("routes", ["routes"]);
  const aliasCounts = new Map<number, number>();
  for (const alias of aliases ?? []) {
    aliasCounts.set(alias.route_id, (aliasCounts.get(alias.route_id) ?? 0) + 1);
  }

  const toggle = useMutation({
    mutationFn: ({ route, enabled }: { route: RouteRecord; enabled: boolean }) => upsertRoute({
      id: route.id,
      name: route.name,
      strategy: route.strategy,
      enabled,
      description: route.description,
      ...(route.settings_json === null ? {} : { settings_json: route.settings_json }),
    }),
    onMutate: async ({ route, enabled }) => {
      await queryClient.cancelQueries({ queryKey: ["routes"] });
      const previous = queryClient.getQueryData<RouteRecord[]>(routesQuery.queryKey);
      queryClient.setQueryData<RouteRecord[]>(routesQuery.queryKey, (current) =>
        current?.map((item) => (item.id === route.id ? { ...item, enabled } : item)),
      );
      queryClient.setQueryData(["routes", route.id], (current: RouteRecord | undefined) =>
        current ? { ...current, enabled } : current,
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) queryClient.setQueryData(routesQuery.queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ["routes"] }),
  });

  return (
    <WorkspaceLayout
      title={t("title")}
      items={rows}
      selectedId={selectedId}
      getId={(route) => route.id}
      getSearchText={(route) => `${route.name} ${route.description ?? ""} ${route.strategy}`}
      renderTitle={(route) => route.name}
      renderSummary={(route) => (
        <span className="truncate">
          {t(`strategy.${route.strategy}`, { defaultValue: route.strategy })} · {t("summary.aliasCount", { count: aliasCounts.get(route.id) ?? 0 })} · {route.enabled ? t("status.enabled") : t("status.disabled")}
        </span>
      )}
      renderLink={(route, content, className) => (
        <Link to="/routes/$routeId" params={{ routeId: String(route.id) }} search={{ tab: "settings" }} className={className}>
          {content}
        </Link>
      )}
      renderAction={(route) => (
        <Switch
          size="sm"
          checked={route.enabled}
          disabled={toggle.isPending}
          aria-label={route.enabled ? t("disable") : t("enable")}
          onCheckedChange={(enabled) => toggle.mutate({ route, enabled })}
        />
      )}
      searchPlaceholder={t("search")}
      emptyLabel={t("empty")}
      createAction={(
        <Button asChild size="icon-sm" aria-label={t("new")}>
          <Link to="/routes/new"><Plus aria-hidden /></Link>
        </Button>
      )}
      batch={{
        active: batch.mode,
        selectedIds: batch.selected as ReadonlySet<number>,
        entryLabel: tc("batch.select"),
        cancelLabel: tc("batch.cancel"),
        selectAllLabel: t("selectAll"),
        onEnter: () => batch.setMode(true),
        onExit: batch.exit,
        onToggle: batch.toggle,
        onToggleAll: batch.toggleAllFor,
        footer: (
          <WorkspaceBatchBar
            count={batch.selected.size}
            pending={batch.pending}
            onEnable={batch.runEnable}
            onDisable={batch.runDisable}
            onDelete={batch.runDelete}
          />
        ),
      }}
      mobileBack={(
        <Link to="/routes" className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <ArrowLeft className="size-4" aria-hidden />{t("title")}
        </Link>
      )}
      emptyState={(
        <div className="hidden h-full min-h-[28rem] place-content-center text-center text-muted-foreground md:grid">
          <Network className="mx-auto mb-3 size-8" aria-hidden />
          <p className="text-sm">{t("selectHint")}</p>
        </div>
      )}
    >
      <Outlet />
    </WorkspaceLayout>
  );
}
