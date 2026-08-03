import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, useNavigate, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Plus, Plug } from "lucide-react";
import { useTranslation } from "react-i18next";
import { providersQuery } from "@/api/providers";
import { credentialModelStatusesQuery, credentialStatusesQuery } from "@/api/usage";
import { providerHealthLevels, ProviderSummary } from "@/components/providers/provider-summary";
import { useProviderToggle } from "@/components/providers/use-provider-toggle";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { WorkspaceBatchBar } from "@/components/workspace/workspace-batch-bar";
import { WorkspaceLayout } from "@/components/workspace/workspace-layout";
import { useBatch } from "@/hooks/use-batch";
import { useChannelCatalog } from "@/hooks/use-channel-catalog";

export const Route = createFileRoute("/_app/providers")({
  loader: ({ context }) => context.queryClient.ensureQueryData(providersQuery),
  component: ProvidersWorkspace,
});

function ProvidersWorkspace() {
  const { t } = useTranslation("providers");
  const { t: tc } = useTranslation("common");
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const selectedId = /(?:^|\/)providers\/([^/]+)/.exec(pathname)?.[1] ?? null;
  const { data: providers } = useQuery(providersQuery);
  const { data: statuses = [] } = useQuery(credentialStatusesQuery);
  const { data: modelStatuses = [] } = useQuery(credentialModelStatusesQuery);
  const catalogState = useChannelCatalog();
  const rows = providers ?? [];
  const health = useMemo(
    () => providerHealthLevels(statuses, modelStatuses),
    [modelStatuses, statuses],
  );
  const toggle = useProviderToggle();
  const batch = useBatch("providers", ["providers"], {
    onSuccess: (operation, ids, outcome) => {
      if (operation !== "delete" || selectedId === null || selectedId === "new") return;
      const selected = Number(selectedId);
      const deleted = ids.some((id) => Number(id) === selected)
        && !outcome.errors.some((error) => error.id === selected);
      if (deleted) void navigate({ to: "/providers" });
    },
  });
  const canCreate = catalogState.authoritative && catalogState.catalog.length > 0;

  return (
    <WorkspaceLayout
      title={t("title")}
      items={rows}
      selectedId={selectedId}
      getId={(provider) => provider.id}
      getSearchText={(provider) => `${provider.label ?? ""} ${provider.name} ${provider.channel}`}
      renderTitle={(provider) => provider.label ?? provider.name}
      renderSummary={(provider) => (
        <ProviderSummary
          channel={provider.channel}
          credentialCount={provider.credential_count}
          level={health.get(provider.id) ?? "healthy"}
        />
      )}
      renderLink={(provider, content, className) => (
        <Link
          to="/providers/$providerId/credentials"
          params={{ providerId: String(provider.id) }}
          className={className}
        >
          {content}
        </Link>
      )}
      renderAction={(provider) => (
        <Switch
          size="sm"
          checked={provider.enabled}
          disabled={toggle.isPending}
          aria-label={provider.enabled ? t("workspace.disable") : t("workspace.enable")}
          onClick={(event) => event.stopPropagation()}
          onCheckedChange={(enabled) => toggle.mutate({ provider, enabled })}
        />
      )}
      searchPlaceholder={t("workspace.search")}
      emptyLabel={t("empty")}
      createAction={canCreate ? (
        <Button asChild size="icon-sm" aria-label={t("new")}>
          <Link to="/providers/new"><Plus aria-hidden /></Link>
        </Button>
      ) : (
        <Button size="icon-sm" aria-label={t("new")} disabled><Plus aria-hidden /></Button>
      )}
      filters={<CatalogNotice state={catalogState} />}
      batch={{
        active: batch.mode,
        selectedIds: batch.selected as ReadonlySet<number>,
        entryLabel: tc("batch.select"),
        cancelLabel: tc("batch.cancel"),
        selectAllLabel: t("workspace.selectAll"),
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
        <Link to="/providers" className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <ArrowLeft className="size-4" aria-hidden />{t("title")}
        </Link>
      )}
      emptyState={(
        <div className="hidden h-full min-h-[28rem] place-content-center text-center text-muted-foreground md:grid">
          <Plug className="mx-auto mb-3 size-8" aria-hidden />
          <p className="text-sm">{t("workspace.selectHint")}</p>
        </div>
      )}
    >
      <Outlet />
    </WorkspaceLayout>
  );
}

function CatalogNotice({ state }: { state: ReturnType<typeof useChannelCatalog> }) {
  const { t } = useTranslation("providers");
  if (state.availability === "ready" && state.catalog.length > 0) return null;
  const key = state.availability === "ready" ? "catalog.empty" : `catalog.${state.availability}`;
  return (
    <div className="grid gap-2 text-xs text-muted-foreground">
      <span className={state.availability === "error" ? "text-destructive" : undefined}>
        {t(key)}
        {state.availability === "error" && state.error instanceof Error ? ` ${state.error.message}` : ""}
      </span>
      {state.availability !== "loading" && state.availability !== "ready" && (
        <Button size="xs" variant="outline" disabled={state.isFetching} onClick={() => void state.refetch()}>
          {t("catalog.retry")}
        </Button>
      )}
    </div>
  );
}
