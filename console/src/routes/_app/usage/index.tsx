import { useMemo, useState } from "react";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { usageInfiniteQuery, type Usage, type UsageFilter } from "@/api/usage";
import { providersQuery } from "@/api/providers";
import { DataTable, type DataColumn } from "@/components/data-table";
import { UsageFilters } from "@/components/observability/usage-filters";
import { RequestDrawer } from "@/components/observability/request-drawer";
import { AuditTab } from "@/components/observability/audit-tab";
import { LogsTab } from "@/components/observability/logs-tab";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

type ExplorerFilter = Omit<UsageFilter, "before_id" | "limit">;

export const Route = createFileRoute("/_app/usage/")({
  loader: ({ context }) => {
    void context.queryClient.ensureQueryData(providersQuery);
  },
  component: UsagePage,
});

function fmtAt(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function UsagePage() {
  const { t } = useTranslation("observability");

  const [filter, setFilter] = useState<ExplorerFilter>({});
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [selectedRid, setSelectedRid] = useState<string | null>(null);

  const { data: providers } = useQuery(providersQuery);
  const providerMap = useMemo(
    () => new Map((providers ?? []).map((p) => [p.id, p.label ?? p.name])),
    [providers],
  );

  // useInfiniteQuery keys on `filter`, so changing it starts a fresh query
  // (pages reset for free); fetchNextPage appends and survives refetches.
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isPending } =
    useInfiniteQuery(usageInfiniteQuery(filter));
  const rows = data?.pages.flat() ?? [];

  function openRid(rid: string) {
    setSelectedRid(rid);
    setDrawerOpen(true);
  }

  const usageCols: DataColumn<Usage>[] = [
    {
      key: "at",
      header: t("usage.columns.at"),
      cell: (r) => (
        <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">
          {fmtAt(r.at)}
        </span>
      ),
    },
    {
      key: "operation",
      header: t("usage.columns.operation"),
      cell: (r) => (
        <span className="font-mono text-xs">
          {r.operation}
          {r.kind && r.kind !== r.operation && (
            <span className="text-muted-foreground"> / {r.kind}</span>
          )}
        </span>
      ),
    },
    {
      key: "model",
      header: t("usage.columns.model"),
      cell: (r) => <span className="font-mono text-xs">{r.model ?? "—"}</span>,
    },
    {
      key: "provider",
      header: t("usage.columns.provider"),
      cell: (r) =>
        r.provider_id != null
          ? (providerMap.get(r.provider_id) ?? `#${r.provider_id}`)
          : "—",
    },
    {
      key: "tokens",
      header: `${t("usage.columns.inputTokens")} / ${t("usage.columns.outputTokens")}`,
      cell: (r) => (
        <span className="tabular-nums text-xs">
          {r.input_tokens} / {r.output_tokens}
        </span>
      ),
    },
    {
      key: "cost",
      header: t("usage.columns.cost"),
      cell: (r) => (
        <span className="tabular-nums text-xs">
          ${parseFloat(r.cost || "0").toFixed(5)}
        </span>
      ),
    },
    {
      key: "latency",
      header: t("usage.columns.latency"),
      cell: (r) => <span className="tabular-nums text-xs">{r.latency_ms}ms</span>,
    },
    {
      key: "badges",
      header: "",
      cell: (r) => (
        <div className="flex gap-1">
          <Badge variant="outline" className="text-xs">{r.usage_source}</Badge>
          <Badge variant="secondary" className="text-xs">{r.ended}</Badge>
        </div>
      ),
    },
  ];

  return (
    <div className="grid gap-4 p-4 md:p-6">
      <h1 className="text-xl font-semibold">{t("statistics")}</h1>

      <Tabs defaultValue="usage">
        <TabsList>
          <TabsTrigger value="usage">{t("usage.tab.usage")}</TabsTrigger>
          <TabsTrigger value="logs">{t("usage.tab.logs")}</TabsTrigger>
          <TabsTrigger value="audit">{t("usage.tab.audit")}</TabsTrigger>
        </TabsList>

        <TabsContent value="usage" className="mt-4 space-y-4">
          <UsageFilters value={filter} onChange={setFilter} />

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
              onRowClick={(r) => openRid(r.request_id)}
              renderCard={(r) => (
                <div className="grid gap-1">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-mono text-xs">{r.operation}</span>
                    <span className="tabular-nums text-xs">${parseFloat(r.cost || "0").toFixed(5)}</span>
                  </div>
                  <div className="flex flex-wrap gap-1 text-xs text-muted-foreground">
                    <span>{fmtAt(r.at)}</span>
                    {r.model && <span>· {r.model}</span>}
                    {r.provider_id != null && <span>· {providerMap.get(r.provider_id) ?? `#${r.provider_id}`}</span>}
                    <span>· {r.latency_ms}ms</span>
                  </div>
                  <div className="flex gap-1">
                    <Badge variant="outline" className="text-xs">{r.usage_source}</Badge>
                    <Badge variant="secondary" className="text-xs">{r.ended}</Badge>
                  </div>
                </div>
              )}
            />
          )}

          {hasNextPage && (
            <div className="flex justify-center pt-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void fetchNextPage()}
                disabled={isFetchingNextPage}
              >
                {isFetchingNextPage ? "…" : t("usage.loadMore")}
              </Button>
            </div>
          )}
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
