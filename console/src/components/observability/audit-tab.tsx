import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { auditQuery, type AuditLog } from "@/api/usage";
import { DataTable, type DataColumn } from "@/components/data-table";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";

function fmtAt(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function AuditTab() {
  const { t } = useTranslation("observability");
  const [auditLimit, setAuditLimit] = useState(100);
  const { data: auditRows, isPending: auditPending } = useQuery(auditQuery(auditLimit));

  const auditCols: DataColumn<AuditLog>[] = [
    {
      key: "at",
      header: t("audit.columns.at"),
      cell: (r) => (
        <span className="whitespace-nowrap font-mono text-xs text-muted-foreground">
          {fmtAt(r.at)}
        </span>
      ),
    },
    { key: "actor", header: t("audit.columns.actor"), cell: (r) => r.actor_name ?? "—" },
    { key: "action", header: t("audit.columns.action"), cell: (r) => <span className="font-mono text-xs">{r.action}</span> },
    { key: "target", header: t("audit.columns.target"), cell: (r) => <span className="font-mono text-xs">{r.target}</span> },
    {
      key: "status",
      header: t("audit.columns.status"),
      cell: (r) => <Badge variant={r.status >= 400 ? "destructive" : "secondary"}>{r.status}</Badge>,
    },
    { key: "ip", header: t("audit.columns.sourceIp"), cell: (r) => r.source_ip ?? "—" },
  ];

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <span className="text-sm text-muted-foreground">{t("audit.limit.label")}</span>
        {(["100", "500", "1000"] as const).map((n) => (
          <button
            key={n}
            type="button"
            onClick={() => setAuditLimit(Number(n))}
            className={
              auditLimit === Number(n)
                ? "rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
                : "rounded-md border px-3 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-accent-foreground"
            }
          >
            {t(`audit.limit.${n}`)}
          </button>
        ))}
      </div>

      {auditPending ? (
        <div className="space-y-2" aria-busy="true">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-10" />
          ))}
        </div>
      ) : (
        <DataTable
          columns={auditCols}
          rows={auditRows ?? []}
          rowKey={(r) => r.id}
          empty={t("audit.empty")}
          renderCard={(r) => (
            <div className="grid gap-1">
              <div className="flex items-center justify-between gap-2">
                <span className="font-mono text-xs">{r.action}</span>
                <Badge variant={r.status >= 400 ? "destructive" : "secondary"}>{r.status}</Badge>
              </div>
              <div className="text-xs text-muted-foreground">
                <span>{fmtAt(r.at)}</span>
                {r.actor_name && <span> · {r.actor_name}</span>}
                {r.source_ip && <span> · {r.source_ip}</span>}
              </div>
              <span className="font-mono text-xs text-muted-foreground">{r.target}</span>
            </div>
          )}
        />
      )}
    </div>
  );
}
