import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { logsPageQuery, type DownstreamRequest, type UsageFilter } from "@/api/usage";
import { DataTable, type DataColumn } from "@/components/data-table";
import { UsageFilters } from "@/components/observability/usage-filters";
import { Pagination } from "@/components/pagination";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";

function fmtAt(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit",
  });
}

/** Recent proxied requests (downstream logs). A row opens the shared request
 *  drawer (downstream + upstream detail) via `onSelect`. */
export function LogsTab({ onSelect }: { onSelect: (requestId: string) => void }) {
  const { t } = useTranslation("observability");
  const [filter, setFilter] = useState<Omit<UsageFilter, "before_id" | "limit">>({});
  const [page, setPage] = useState(1);
  const { data, isFetching, isPending } = useQuery(logsPageQuery(filter, page));
  const rows = data?.items ?? [];

  function changeFilter(next: Omit<UsageFilter, "before_id" | "limit">) {
    setPage(1);
    setFilter(next);
  }

  const cols: DataColumn<DownstreamRequest>[] = [
    {
      key: "at",
      header: t("logsList.columns.at"),
      cell: (r) => <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">{fmtAt(r.at)}</span>,
    },
    { key: "method", header: t("logsList.columns.method"), cell: (r) => <span className="font-mono text-xs">{r.method}</span> },
    {
      key: "path",
      header: t("logsList.columns.path"),
      cell: (r) => <span className="font-mono text-xs">{r.path}{r.query ? `?${r.query}` : ""}</span>,
    },
    {
      key: "status",
      header: t("logsList.columns.status"),
      cell: (r) => <Badge variant={r.status >= 400 ? "destructive" : "secondary"}>{r.status}</Badge>,
    },
  ];

  if (isPending) {
    return (
      <div className="space-y-2" aria-busy="true">
        {Array.from({ length: 5 }).map((_, i) => <Skeleton key={i} className="h-10" />)}
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <UsageFilters
        value={filter}
        onChange={changeFilter}
        showModel={false}
        routeListId="logs-route-datalist"
      />
      <DataTable
        columns={cols}
        rows={rows}
        rowKey={(r) => r.id}
        empty={t("logsList.empty")}
        onRowClick={(r) => onSelect(r.request_id)}
        renderCard={(r) => (
          <div className="grid gap-1">
            <div className="flex items-center justify-between gap-2">
              <span className="font-mono text-xs">
                {r.method} {r.path}{r.query ? `?${r.query}` : ""}
              </span>
              <Badge variant={r.status >= 400 ? "destructive" : "secondary"}>{r.status}</Badge>
            </div>
            <span className="text-xs text-muted-foreground">{fmtAt(r.at)}</span>
          </div>
        )}
      />
      <Pagination
        page={page}
        totalPages={data?.pagination.total_pages ?? 0}
        onPageChange={setPage}
        disabled={isFetching}
      />
    </div>
  );
}
