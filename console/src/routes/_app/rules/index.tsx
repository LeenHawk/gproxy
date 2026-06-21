import { useState } from "react";
import { useQuery, useQueryClient, useQueries } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { Copy, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ruleSetsQuery, providerRuleSetsQuery, cloneRuleSet, type RuleSet } from "@/api/rules";
import { providersQuery } from "@/api/providers";
import { computeRuleSetUsage } from "@/lib/rule-usage";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { RuleSetForm } from "@/components/rules/rule-set-form";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/rules/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(ruleSetsQuery),
  component: RuleSetsPage,
});

type ScopeFilter = "all" | "private" | "shared";

function UsageBadge({ ruleSetId, allAttachments, provName }: {
  ruleSetId: number;
  allAttachments: { rule_set_id: number; provider_id: number }[];
  provName: Map<number, string>;
}) {
  const { t } = useTranslation("rules");
  const u = computeRuleSetUsage(ruleSetId, allAttachments as Parameters<typeof computeRuleSetUsage>[1]);
  const variant = u.scope === "unused" ? "outline" : u.scope === "shared" ? "default" : "secondary";
  const label =
    u.scope === "shared"
      ? t("usage.shared", { count: u.providerIds.length })
      : t(`usage.${u.scope}`);
  const title =
    u.scope !== "unused"
      ? `${t("usage.usedBy")}: ${u.providerIds.map((id) => provName.get(id) ?? String(id)).join(", ")}`
      : undefined;
  return <Badge variant={variant} title={title}>{label}</Badge>;
}

function RuleSetsPage() {
  const { t } = useTranslation("rules");
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: ruleSets, isPending } = useQuery(ruleSetsQuery);
  const { data: providers = [] } = useQuery(providersQuery);
  const [createOpen, setCreateOpen] = useState(false);
  const [scopeFilter, setScopeFilter] = useState<ScopeFilter>("all");
  const [cloning, setCloning] = useState<number | null>(null);

  const attachQueries = useQueries({ queries: providers.map((p) => providerRuleSetsQuery(p.id)) });
  const allAttachments = attachQueries.flatMap((q) => q.data ?? []);
  const provName = new Map(providers.map((p) => [p.id, p.label ?? p.name]));

  const rows = (ruleSets ?? []).filter((r) =>
    scopeFilter === "all" ? true : computeRuleSetUsage(r.id, allAttachments).scope === scopeFilter,
  );

  async function handleClone(r: RuleSet, e: React.MouseEvent) {
    e.stopPropagation();
    setCloning(r.id);
    try {
      const copy = await cloneRuleSet(r, t("usage.cloneSuffix"));
      await queryClient.invalidateQueries({ queryKey: ["rule-sets"] });
      void navigate({ to: "/rules/$ruleSetId", params: { ruleSetId: String(copy.id) } });
    } finally {
      setCloning(null);
    }
  }

  const columns: DataColumn<RuleSet>[] = [
    { key: "name", header: t("ruleSet.name"), cell: (r) => <span className="font-medium">{r.name}</span> },
    { key: "enabled", header: t("ruleSet.enabled"), cell: (r) => (
      <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? "on" : "off"}</Badge>
    )},
    { key: "usage", header: t("usage.usedBy"), cell: (r) => (
      <UsageBadge ruleSetId={r.id} allAttachments={allAttachments} provName={provName} />
    )},
    { key: "description", header: t("ruleSet.description"), cell: (r) => (
      <span className="max-w-xs truncate text-sm text-muted-foreground">{r.description ?? "—"}</span>
    )},
    { key: "clone", header: "", cell: (r) => (
      <Button size="sm" variant="ghost" disabled={cloning === r.id} onClick={(e) => void handleClone(r, e)}
        title={t("usage.clone")}>
        <Copy className="size-4" aria-hidden />
        <span className="sr-only">{t("usage.clone")}</span>
      </Button>
    )},
  ];

  const scopeButtons: { key: ScopeFilter; label: string }[] = [
    { key: "all", label: t("usage.filterAll") },
    { key: "private", label: t("usage.filterPrivate") },
    { key: "shared", label: t("usage.filterShared") },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold">{t("title")}</h1>
          <p className="text-sm text-muted-foreground">{t("subtitle")}</p>
        </div>
        <Button onClick={() => setCreateOpen(true)}>
          <Plus className="size-4" aria-hidden />
          <span className="hidden sm:inline">{t("ruleSet.add")}</span>
        </Button>
      </div>
      <div className="flex gap-1">
        {scopeButtons.map(({ key, label }) => (
          <Button key={key} size="sm" variant={scopeFilter === key ? "default" : "outline"}
            onClick={() => setScopeFilter(key)}>
            {label}
          </Button>
        ))}
      </div>
      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={rows}
          rowKey={(r) => r.id}
          empty={t("ruleSet.empty")}
          onRowClick={(r) => void navigate({ to: "/rules/$ruleSetId", params: { ruleSetId: String(r.id) } })}
          renderCard={(r) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between">
                <span className="font-medium">{r.name}</span>
                <div className="flex items-center gap-1">
                  <UsageBadge ruleSetId={r.id} allAttachments={allAttachments} provName={provName} />
                  <Badge variant={r.enabled ? "secondary" : "outline"}>{r.enabled ? "on" : "off"}</Badge>
                  <Button size="sm" variant="ghost" disabled={cloning === r.id}
                    onClick={(e) => void handleClone(r, e)} title={t("usage.clone")}>
                    <Copy className="size-3" aria-hidden />
                  </Button>
                </div>
              </div>
              {r.description && (
                <p className="truncate text-xs text-muted-foreground">{r.description}</p>
              )}
            </div>
          )}
        />
      )}
      <EntityDialog open={createOpen} onOpenChange={setCreateOpen} title={t("ruleSet.add")}>
        <RuleSetForm onSaved={(saved) => {
          setCreateOpen(false);
          void navigate({ to: "/rules/$ruleSetId", params: { ruleSetId: String(saved.id) } });
        }} />
      </EntityDialog>
    </div>
  );
}
