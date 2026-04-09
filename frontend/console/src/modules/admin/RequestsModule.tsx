import { useI18n } from "../../app/i18n";
import { Card } from "../../components/ui";
import { RequestFilters } from "./requests/RequestFilters";
import { RequestsTable } from "./requests/RequestsTable";
import { useRequestsModuleState } from "./requests/useRequestsModuleState";

/// Thin shell for the Admin > Requests page. All state, filter options, and
/// IO live in `useRequestsModuleState`; presentation is split across
/// `RequestFilters`, `RequestsTable`, and `PayloadCell` so each piece can be
/// iterated on independently. This mirrors the sample gproxy module layout.
export function RequestsModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const state = useRequestsModuleState({ sessionToken, notify, t });

  return (
    <Card title={t("requests.title")} subtitle={t("requests.subtitle")}>
      <div className="mb-3 text-xs text-muted">{t("requests.bodyHint")}</div>
      <RequestFilters
        kind={state.kind}
        onKindChange={state.setKind}
        filters={state.filters}
        onFilterChange={state.updateFilter}
        providerOptions={state.providerOptions}
        credentialOptions={state.filteredCredentialOptions}
        userOptions={state.userOptions}
        userKeyOptions={state.filteredUserKeyOptions}
        requestPathOptions={state.requestPathOptions}
        isFilterOptionsLoading={state.isFilterOptionsLoading}
        loadingRows={state.loadingRows}
        loadingCount={state.loadingCount}
        clearingPayload={state.clearingPayload}
        deletingLogs={state.deletingLogs}
        selectedCount={state.selectedTraceIds.length}
        onRunQuery={state.runQuery}
        onClearPayload={(all) => void state.clearPayload(all)}
        onDeleteLogs={(all) => void state.deleteLogs(all)}
        t={t}
      />
      <RequestsTable
        kind={state.kind}
        rows={state.rows}
        bodyByTraceId={state.bodyByTraceId}
        bodyLoadingByTraceId={state.bodyLoadingByTraceId}
        bodyErrorByTraceId={state.bodyErrorByTraceId}
        ensureBodyLoaded={state.ensureBodyLoaded}
        selectedTraceIds={state.selectedTraceIds}
        clearingPayload={state.clearingPayload}
        deletingLogs={state.deletingLogs}
        onToggleTraceIdSelected={state.toggleTraceIdSelected}
        totalRows={state.totalRows}
        pageSize={state.pageSize}
        onPageSizeChange={state.setPageSize}
        page={state.page}
        totalPages={state.totalPages}
        canGoNext={state.canGoNext}
        loadingRows={state.loadingRows}
        loadingCount={state.loadingCount}
        onPageChange={state.setPage}
        notify={notify}
        t={t}
      />
    </Card>
  );
}
