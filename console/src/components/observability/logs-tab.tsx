import { useInfiniteQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { logsInfiniteQuery, type DownstreamRequest } from "@/api/usage";
import { DataTable, type DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
  const { data, fetchNextPage, hasNextPage, isFetchingNextPage, isPending } =
    useInfiniteQuery(logsInfiniteQuery());
  const rows = data?.pages.flat() ?? [];

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
      <DataTable
        columns={cols}
        rows={rows}
        rowKey={(r) => r.id}
        empty={t("logsList.empty")}
        onRowClick={(r) => onSelect(r.request_id)}
        renderCard={(r) => (
          <div className="grid gap-1">
            <div className="flex items-center justify-between gap-2">
              <span className="font-mono text-xs">{r.method} {r.path}</span>
              <Badge variant={r.status >= 400 ? "destructive" : "secondary"}>{r.status}</Badge>
            </div>
            <span className="text-xs text-muted-foreground">{fmtAt(r.at)}</span>
          </div>
        )}
      />
      {hasNextPage && (
        <div className="flex justify-center pt-2">
          <Button variant="outline" size="sm" onClick={() => void fetchNextPage()} disabled={isFetchingNextPage}>
            {isFetchingNextPage ? "…" : t("usage.loadMore")}
          </Button>
        </div>
      )}
    </div>
  );
}
