import { formatAtForViewer } from "../../../lib/datetime";
import type {
  DownstreamRequestQueryRow,
  UpstreamRequestQueryRow,
} from "../../../lib/types/admin";
import { Button, Select, Table } from "../../../components/ui";
import { PayloadCell } from "./payload";
import type {
  NotifyFn,
  RequestBodyPayload,
  RequestKind,
  RequestRow,
  TranslateFn,
} from "./types";

/// Renders the request rows for one kind (upstream or downstream) with a
/// select column, cursor-aware pagination, and an expandable payload cell
/// per row. All state lives in the caller (`useRequestsModuleState`); this
/// component is a pure view.
export function RequestsTable({
  kind,
  rows,
  bodyByTraceId,
  bodyLoadingByTraceId,
  bodyErrorByTraceId,
  ensureBodyLoaded,
  selectedTraceIds,
  clearingPayload,
  deletingLogs,
  onToggleTraceIdSelected,
  totalRows,
  pageSize,
  onPageSizeChange,
  page,
  totalPages,
  canGoNext,
  loadingRows,
  loadingCount,
  onPageChange,
  notify,
  t,
}: {
  kind: RequestKind;
  rows: RequestRow[];
  bodyByTraceId: Record<number, RequestBodyPayload>;
  bodyLoadingByTraceId: Record<number, boolean>;
  bodyErrorByTraceId: Record<number, string>;
  ensureBodyLoaded: (row: RequestRow) => Promise<RequestBodyPayload | undefined>;
  selectedTraceIds: number[];
  clearingPayload: boolean;
  deletingLogs: boolean;
  onToggleTraceIdSelected: (traceId: number) => void;
  totalRows: number;
  pageSize: number;
  onPageSizeChange: (pageSize: number) => void;
  page: number;
  totalPages: number;
  canGoNext: boolean;
  loadingRows: boolean;
  loadingCount: boolean;
  onPageChange: (page: number) => void;
  notify: NotifyFn;
  t: TranslateFn;
}) {
  const traceIdColumn = t("table.trace_id");
  const atColumn = t("table.at");
  const statusColumn = t("table.status");
  const requestPathColumn = kind === "upstream" ? t("table.url") : t("table.path");
  const credentialIdColumn = t("field.credential_id");
  const methodColumn = t("table.method");
  const payloadColumn = t("table.payload");
  const selectColumn = t("requests.clear.selectRow");

  const tableColumns =
    kind === "upstream"
      ? [
          selectColumn,
          traceIdColumn,
          atColumn,
          statusColumn,
          requestPathColumn,
          credentialIdColumn,
          methodColumn,
          payloadColumn,
        ]
      : [
          selectColumn,
          traceIdColumn,
          atColumn,
          statusColumn,
          requestPathColumn,
          methodColumn,
          payloadColumn,
        ];

  return (
    <div className="mt-4">
      <Table
        columns={tableColumns}
        rows={rows.map((row) => {
          const payloadCell = (
            <PayloadCell
              row={row}
              t={t}
              notify={notify}
              detail={bodyByTraceId[row.trace_id]}
              loadingBody={Boolean(bodyLoadingByTraceId[row.trace_id])}
              bodyError={bodyErrorByTraceId[row.trace_id]}
              ensureBodyLoaded={ensureBodyLoaded}
            />
          );
          const selected = selectedTraceIds.includes(row.trace_id);
          const selectCell = (
            <label className="inline-flex cursor-pointer items-center justify-center">
              <input
                type="checkbox"
                checked={selected}
                disabled={clearingPayload || deletingLogs}
                onChange={() => onToggleTraceIdSelected(row.trace_id)}
                aria-label={
                  selected ? t("requests.clear.unselectRow") : t("requests.clear.selectRow")
                }
                className="h-4 w-4"
              />
            </label>
          );

          if (kind === "upstream") {
            const upstreamRow = row as UpstreamRequestQueryRow;
            return {
              [selectColumn]: selectCell,
              [traceIdColumn]: upstreamRow.downstream_trace_id ?? row.trace_id,
              [atColumn]: formatAtForViewer(row.at),
              [statusColumn]: row.response_status ?? "",
              [requestPathColumn]: upstreamRow.request_url ?? "",
              [credentialIdColumn]: upstreamRow.credential_id ?? "",
              [methodColumn]: row.request_method,
              [payloadColumn]: payloadCell,
            };
          }

          const downstreamRow = row as DownstreamRequestQueryRow;
          return {
            [selectColumn]: selectCell,
            [traceIdColumn]: row.trace_id,
            [atColumn]: formatAtForViewer(row.at),
            [statusColumn]: row.response_status ?? "",
            [requestPathColumn]: downstreamRow.request_path,
            [methodColumn]: row.request_method,
            [payloadColumn]: payloadCell,
          };
        })}
      />
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
        <div>
          {t("common.pager.stats", { shown: rows.length, total: totalRows })}
        </div>
        <div className="flex items-center gap-2">
          <span>{t("common.show")}</span>
          <div className="w-20">
            <Select
              value={String(pageSize)}
              onChange={(value) => onPageSizeChange(Number(value))}
              options={[
                { value: "5", label: "5" },
                { value: "10", label: "10" },
                { value: "20", label: "20" },
                { value: "50", label: "50" },
              ]}
            />
          </div>
          <Button
            variant="neutral"
            disabled={page <= 1 || loadingRows}
            onClick={() => onPageChange(Math.max(1, page - 1))}
          >
            {t("common.pager.prev")}
          </Button>
          <span>{t("common.pager.page", { current: page, total: totalPages })}</span>
          <Button
            variant="neutral"
            disabled={!canGoNext || loadingRows || loadingCount}
            onClick={() => onPageChange(Math.min(totalPages, page + 1))}
          >
            {t("common.pager.next")}
          </Button>
        </div>
      </div>
    </div>
  );
}
