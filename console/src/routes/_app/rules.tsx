import { useMemo, useState } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Outlet, createFileRoute, useNavigate, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Copy, Plus, ScrollText } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ApiError } from "@/api/http";
import { providersQuery } from "@/api/providers";
import { cloneRuleSet, providerRuleSetsQuery, ruleSetsQuery, type RuleSet } from "@/api/rules";
import { EnabledToggle } from "@/components/enabled-toggle";
import { RuleSetUsageBadge } from "@/components/rules/rule-set-usage-badge";
import { WorkspaceBatchBar } from "@/components/workspace/workspace-batch-bar";
import { WorkspaceLayout } from "@/components/workspace/workspace-layout";
import { Button } from "@/components/ui/button";
import { useBatch } from "@/hooks/use-batch";
import { useRuleSetToggle } from "@/hooks/use-rule-set-toggle";
import { computeRuleSetUsage } from "@/lib/rule-usage";

export const Route = createFileRoute("/_app/rules")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(providersQuery);
    return context.queryClient.ensureQueryData(ruleSetsQuery);
  },
  component: RulesWorkspace,
});

type ScopeFilter = "all" | "private" | "shared";

function RulesWorkspace() {
  const { t } = useTranslation("rules");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const selectedId = /(?:^|\/)rules\/([^/]+)/.exec(pathname)?.[1] ?? null;
  const { data: ruleSets = [] } = useQuery(ruleSetsQuery);
  const { data: providers = [] } = useQuery(providersQuery);
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>("all");
  const batch = useBatch("rule-sets", ["rule-sets"]);
  const toggle = useRuleSetToggle();

  // Keep the existing per-provider attachment queries mounted in the layout so
  // selecting a detail route does not remount/refetch the usage badge data.
  const attachmentQueries = useQueries({ queries: providers.map((provider) => providerRuleSetsQuery(provider.id)) });
  const attachments = attachmentQueries.flatMap((query) => query.data ?? []);
  const providerNames = useMemo(
    () => new Map(providers.map((provider) => [provider.id, provider.label ?? provider.name])),
    [providers],
  );
  const usageById = new Map(ruleSets.map((ruleSet) => [ruleSet.id, computeRuleSetUsage(ruleSet.id, attachments)]));
  const rows = ruleSets.filter((ruleSet) => {
    const scope = usageById.get(ruleSet.id)?.scope;
    return scopeFilter === "all" || scope === scopeFilter;
  });

  const clone = useMutation({
    mutationFn: (ruleSet: RuleSet) => cloneRuleSet(ruleSet, t("usage.cloneSuffix")),
    onSuccess: async (copy) => {
      await queryClient.invalidateQueries({ queryKey: ["rule-sets"] });
      void navigate({ to: "/rules/$ruleSetId", params: { ruleSetId: String(copy.id) } });
    },
    onError: (error) => toast.error(error instanceof ApiError ? error.message : String(error)),
  });

  const scopeFilters: { key: ScopeFilter; label: string }[] = [
    { key: "all", label: t("usage.filterAll") },
    { key: "private", label: t("usage.filterPrivate") },
    { key: "shared", label: t("usage.filterShared") },
  ];

  return (
    <WorkspaceLayout
      storageKey="gproxy.workspace.rules.width"
      title={t("title")}
      items={rows}
      selectedId={selectedId}
      getId={(ruleSet) => ruleSet.id}
      getSearchText={(ruleSet) => `${ruleSet.name} ${ruleSet.description ?? ""}`}
      renderTitle={(ruleSet) => ruleSet.name}
      renderSummary={(ruleSet) => (
        <>
          <span className="shrink-0">{ruleSet.enabled ? t("status.enabled") : t("status.disabled")}</span>
          <span aria-hidden>·</span>
          <RuleSetUsageBadge usage={usageById.get(ruleSet.id)!} providerNames={providerNames} />
          {ruleSet.description && <><span aria-hidden>·</span><span className="truncate">{ruleSet.description}</span></>}
        </>
      )}
      renderLink={(ruleSet, content, className) => (
        <Link to="/rules/$ruleSetId" params={{ ruleSetId: String(ruleSet.id) }} className={className}>{content}</Link>
      )}
      renderAction={(ruleSet) => (
        <div className="flex items-center gap-1">
          <EnabledToggle
            enabled={ruleSet.enabled}
            pending={toggle.isPending}
            onToggle={(enabled) => toggle.mutate({ ruleSet, enabled })}
          />
          <Button
            size="icon-xs"
            variant="ghost"
            disabled={clone.isPending && clone.variables?.id === ruleSet.id}
            onClick={() => clone.mutate(ruleSet)}
            aria-label={t("usage.clone")}
          >
            <Copy aria-hidden />
          </Button>
        </div>
      )}
      searchPlaceholder={t("workspace.search")}
      emptyLabel={t("ruleSet.empty")}
      filters={(
        <div className="flex gap-1">
          {scopeFilters.map(({ key, label }) => (
            <Button key={key} size="xs" variant={scopeFilter === key ? "default" : "outline"} onClick={() => setScopeFilter(key)}>
              {label}
            </Button>
          ))}
        </div>
      )}
      createAction={(
        <Button asChild size="icon-sm" aria-label={t("ruleSet.add")}>
          <Link to="/rules/new"><Plus aria-hidden /></Link>
        </Button>
      )}
      batch={{
        active: batch.mode,
        selectedIds: batch.selected,
        entryLabel: tc("batch.select"),
        cancelLabel: tc("batch.cancel"),
        selectAllLabel: t("workspace.selectAll"),
        onEnter: () => batch.setMode(true),
        onExit: batch.exit,
        onToggle: batch.toggle,
        onToggleAll: batch.toggleAllFor,
        footer: <WorkspaceBatchBar count={batch.selected.size} pending={batch.pending} onEnable={batch.runEnable} onDisable={batch.runDisable} onDelete={batch.runDelete} />,
      }}
      mobileBack={(
        <Link to="/rules" className="inline-flex items-center gap-2 text-sm text-muted-foreground">
          <ArrowLeft className="size-4" aria-hidden />{t("title")}
        </Link>
      )}
      emptyState={(
        <div className="hidden h-full min-h-[28rem] place-content-center text-center text-muted-foreground md:grid">
          <ScrollText className="mx-auto mb-3 size-8" aria-hidden />
          <p className="text-sm">{t("workspace.selectHint")}</p>
        </div>
      )}
    >
      <Outlet />
    </WorkspaceLayout>
  );
}
