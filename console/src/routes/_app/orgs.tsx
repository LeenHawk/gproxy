import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Building2, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { orgsQuery, upsertOrg, type Org } from "@/api/identity";
import { WorkspaceBatchBar } from "@/components/workspace/workspace-batch-bar";
import { WorkspaceLayout } from "@/components/workspace/workspace-layout";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useBatch } from "@/hooks/use-batch";

export const Route = createFileRoute("/_app/orgs")({
  loader: ({ context }) => context.queryClient.ensureQueryData(orgsQuery),
  component: OrgsWorkspace,
});

function OrgsWorkspace() {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const selectedId = /(?:^|\/)orgs\/([^/]+)/.exec(pathname)?.[1] ?? null;
  const { data: orgs } = useQuery(orgsQuery);
  const rows = orgs ?? [];
  const batch = useBatch("orgs", ["orgs"]);

  const toggle = useMutation({
    mutationFn: ({ org, enabled }: { org: Org; enabled: boolean }) => upsertOrg({
      id: org.id,
      name: org.name,
      description: org.description,
      enabled,
    }),
    onMutate: async ({ org, enabled }) => {
      await queryClient.cancelQueries({ queryKey: ["orgs"] });
      const previous = queryClient.getQueryData<Org[]>(orgsQuery.queryKey);
      queryClient.setQueryData<Org[]>(orgsQuery.queryKey, (current) =>
        current?.map((item) => (item.id === org.id ? { ...item, enabled } : item)),
      );
      queryClient.setQueryData(["orgs", org.id], (current: Org | undefined) =>
        current ? { ...current, enabled } : current,
      );
      return { previous };
    },
    onError: (error, _variables, context) => {
      if (context?.previous) queryClient.setQueryData(orgsQuery.queryKey, context.previous);
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
    onSettled: () => void queryClient.invalidateQueries({ queryKey: ["orgs"] }),
  });

  return (
    <WorkspaceLayout
      title={t("orgs.title")}
      items={rows}
      selectedId={selectedId}
      getId={(org) => org.id}
      getSearchText={(org) => `${org.name} ${org.description ?? ""}`}
      renderTitle={(org) => org.name}
      renderSummary={(org) => (
        <span className="truncate">
          {org.description ?? "—"} · {org.enabled ? t("orgs.status.enabled") : t("orgs.status.disabled")}
        </span>
      )}
      renderLink={(org, content, className) => (
        <Link to="/orgs/$orgId" params={{ orgId: String(org.id) }} search={{ tab: "profile" }} className={className}>
          {content}
        </Link>
      )}
      renderAction={(org) => (
        <Switch
          size="sm"
          checked={org.enabled}
          disabled={toggle.isPending}
          aria-label={org.enabled ? t("orgs.disable") : t("orgs.enable")}
          onCheckedChange={(enabled) => toggle.mutate({ org, enabled })}
        />
      )}
      searchPlaceholder={t("orgs.search")}
      emptyLabel={t("orgs.empty")}
      createAction={(
        <Button asChild size="icon-sm" aria-label={t("orgs.new")}>
          <Link to="/orgs/new"><Plus aria-hidden /></Link>
        </Button>
      )}
      batch={{
        active: batch.mode,
        selectedIds: batch.selected as ReadonlySet<number>,
        entryLabel: tc("batch.select"),
        cancelLabel: tc("batch.cancel"),
        selectAllLabel: t("orgs.selectAll"),
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
        <Link to="/orgs" className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <ArrowLeft className="size-4" aria-hidden />{t("orgs.title")}
        </Link>
      )}
      emptyState={(
        <div className="hidden h-full min-h-[28rem] place-content-center text-center text-muted-foreground md:grid">
          <Building2 className="mx-auto mb-3 size-8" aria-hidden />
          <p className="text-sm">{t("orgs.selectHint")}</p>
        </div>
      )}
    >
      <Outlet />
    </WorkspaceLayout>
  );
}
