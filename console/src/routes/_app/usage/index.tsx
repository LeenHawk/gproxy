import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import {
  usagePageQuery,
  usageSummaryQuery,
  type UsageFilter,
} from "@/api/usage";
import { providersQuery } from "@/api/providers";
import { DataTable } from "@/components/data-table";
import { Pagination } from "@/components/pagination";
import { UsageFilters } from "@/components/observability/usage-filters";
import { UsageSummaryBar } from "@/components/observability/usage-summary-bar";
import { UsageMobileCard } from "@/components/observability/usage-mobile-card";
import { useUsageColumns } from "@/components/observability/usage-columns";
import { RequestDrawer } from "@/components/observability/request-drawer";
import { ClearAllButton } from "@/components/observability/clear-all-button";
import { AuditTab } from "@/components/observability/audit-tab";
import { CredentialUsageComparisonTab } from "@/components/observability/credential-usage-comparison";
import { LogsTab } from "@/components/observability/logs-tab";
import { BatchToolbar } from "@/components/batch-toolbar";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useBatch } from "@/hooks/use-batch";

type ExplorerFilter = Omit<UsageFilter, "before_id" | "limit">;

export const Route = createFileRoute("/_app/usage/")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(providersQuery);
  },
  component: UsagePage,
});

function UsagePage() {
  const { t } = useTranslation("observability");
  const { t: tc } = useTranslation("common");

  const [filter, setFilter] = useState<ExplorerFilter>({});
  const [page, setPage] = useState(1);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [selectedRid, setSelectedRid] = useState<string | null>(null);

  const { data: providers } = useQuery(providersQuery);
  const providerMap = useMemo(
    () => new Map((providers ?? []).map((p) => [p.id, p.label ?? p.name])),
    [providers],
  );
  const usageCols = useUsageColumns(providerMap);

  const { data, isFetching, isPending } = useQuery(usagePageQuery(filter, page));
  const { data: summary, isPending: summaryPending } = useQuery(
    usageSummaryQuery(filter),
  );
  const rows = data?.items ?? [];
  const ids = rows.map((r) => r.id);

  // Batch: usage is read-only — delete only.
  const batch = useBatch("usage", ["usage"]);

  function openRid(rid: string) {
    setSelectedRid(rid);
    setDrawerOpen(true);
  }

  function changeFilter(next: ExplorerFilter) {
    batch.exit();
    setPage(1);
    setFilter(next);
  }

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <h1 className="text-xl font-semibold">{t("statistics")}</h1>

      <Tabs defaultValue="usage">
        <TabsList>
          <TabsTrigger value="usage">{t("usage.tab.usage")}</TabsTrigger>
          <TabsTrigger value="credentials">{t("usage.tab.credentials")}</TabsTrigger>
          <TabsTrigger value="logs">{t("usage.tab.logs")}</TabsTrigger>
          <TabsTrigger value="audit">{t("usage.tab.audit")}</TabsTrigger>
        </TabsList>

        <TabsContent value="usage" className="mt-4 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <UsageFilters
              value={filter}
              onChange={changeFilter}
              showCredential
            />
            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" onClick={() => batch.mode ? batch.exit() : batch.setMode(true)}>
                {batch.mode ? tc("batch.cancel") : tc("batch.select")}
              </Button>
              <ClearAllButton
                data="usage"
                onCleared={() => {
                  batch.exit();
                  setPage(1);
                }}
              />
            </div>
          </div>

          <UsageSummaryBar summary={summary} pending={summaryPending} />

          {isPending ? (
            <div className="space-y-2" aria-busy="true">
              {Array.from({ length: 5 }).map((_, i) => (
                <Skeleton key={i} className="h-10" />
              ))}
            </div>
          ) : (
            <DataTable
              columns={usageCols}
              rows={rows}
              rowKey={(r) => r.id}
              empty={t("usage.empty")}
              columnToggle={{
                storageKey: "gproxy.usage.columns",
                label: t("usage.columnToggle"),
                defaultHidden: ["cache30m"],
              }}
              onRowClick={batch.mode ? undefined : (r) => openRid(r.request_id)}
              selection={batch.mode ? {
                selectedIds: batch.selected,
                onToggle: batch.toggle,
                onToggleAll: () => batch.toggleAllFor(ids),
                allSelected: batch.allSelectedFor(ids),
                indeterminate: batch.selected.size > 0 && !batch.allSelectedFor(ids),
              } : undefined}
              renderCard={(r) => (
                <UsageMobileCard
                  usage={r}
                  providerLabel={
                    r.provider_id != null
                      ? (providerMap.get(r.provider_id) ?? `#${r.provider_id}`)
                      : undefined
                  }
                />
              )}
            />
          )}

          <Pagination
            page={page}
            totalPages={data?.pagination.total_pages ?? 0}
            onPageChange={setPage}
            disabled={isFetching || batch.mode}
          />

          {batch.mode && (
            <BatchToolbar
              count={batch.selected.size}
              enableDisable={false}
              onDelete={batch.runDelete}
              onCancel={batch.exit}
              pending={batch.pending}
            />
          )}
        </TabsContent>

        <TabsContent value="credentials" className="mt-4 space-y-4">
          <CredentialUsageComparisonTab />
        </TabsContent>

        <TabsContent value="logs" className="mt-4 space-y-4">
          <LogsTab onSelect={openRid} />
        </TabsContent>

        <TabsContent value="audit" className="mt-4 space-y-4">
          <AuditTab />
        </TabsContent>
      </Tabs>

      <RequestDrawer
        open={drawerOpen}
        onOpenChange={setDrawerOpen}
        requestId={selectedRid}
      />
    </div>
  );
}
