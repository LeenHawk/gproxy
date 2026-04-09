import { Button, Input, Label, SearchableSelect, Select } from "../../../components/ui";
import type { RequestKind, RequestsFilterState, SelectOption, TranslateFn } from "./types";

/// Filter panel rendered above the requests table. Owns all form inputs
/// but not the state — parent passes `filters` + `onFilterChange` and
/// receives `onRunQuery` / `onClearPayload` / `onDeleteLogs` callbacks.
export function RequestFilters({
  kind,
  onKindChange,
  filters,
  onFilterChange,
  providerOptions,
  credentialOptions,
  userOptions,
  userKeyOptions,
  requestPathOptions,
  isFilterOptionsLoading,
  loadingRows,
  loadingCount,
  clearingPayload,
  deletingLogs,
  selectedCount,
  onRunQuery,
  onClearPayload,
  onDeleteLogs,
  t,
}: {
  kind: RequestKind;
  onKindChange: (kind: RequestKind) => void;
  filters: RequestsFilterState;
  onFilterChange: (key: keyof RequestsFilterState, value: string) => void;
  providerOptions: SelectOption[];
  credentialOptions: SelectOption[];
  userOptions: SelectOption[];
  userKeyOptions: SelectOption[];
  requestPathOptions: SelectOption[];
  isFilterOptionsLoading: boolean;
  loadingRows: boolean;
  loadingCount: boolean;
  clearingPayload: boolean;
  deletingLogs: boolean;
  selectedCount: number;
  onRunQuery: () => void;
  onClearPayload: (all: boolean) => void;
  onDeleteLogs: (all: boolean) => void;
  t: TranslateFn;
}) {
  const busy = loadingRows || loadingCount || clearingPayload || deletingLogs;

  return (
    <>
      <div className="grid gap-3 md:grid-cols-3">
        <div>
          <Label>{t("field.kind")}</Label>
          <Select
            value={kind}
            onChange={(value) => onKindChange(value as RequestKind)}
            options={[
              { value: "downstream", label: t("requests.kind.downstream") },
              { value: "upstream", label: t("requests.kind.upstream") },
            ]}
          />
        </div>
        <div>
          <Label>{t("field.provider_id")}</Label>
          <Select
            value={filters.providerId}
            onChange={(value) => onFilterChange("providerId", value)}
            options={providerOptions}
            disabled={kind !== "upstream" || isFilterOptionsLoading}
          />
        </div>
        <div>
          <Label>{t("field.credential_id")}</Label>
          <Select
            value={filters.credentialId}
            onChange={(value) => onFilterChange("credentialId", value)}
            options={credentialOptions}
            disabled={kind !== "upstream" || isFilterOptionsLoading}
          />
        </div>
        <div>
          <Label>{t("field.user_id")}</Label>
          <SearchableSelect
            value={filters.userId}
            onChange={(value) => onFilterChange("userId", value)}
            options={userOptions}
            placeholder={t("common.all")}
            noResultLabel={t("common.none")}
            disabled={kind !== "downstream" || isFilterOptionsLoading}
          />
        </div>
        <div>
          <Label>{t("field.user_key_id")}</Label>
          <SearchableSelect
            value={filters.userKeyId}
            onChange={(value) => onFilterChange("userKeyId", value)}
            options={userKeyOptions}
            placeholder={t("common.all")}
            noResultLabel={t("common.none")}
            disabled={kind !== "downstream" || isFilterOptionsLoading}
          />
        </div>
        <div>
          <Label>{t("requests.requestPathContains")}</Label>
          <SearchableSelect
            value={filters.requestPathContains}
            onChange={(value) => onFilterChange("requestPathContains", value)}
            options={requestPathOptions}
            placeholder={t("requests.path.placeholder")}
            noResultLabel={t("common.none")}
          />
        </div>
        <div>
          <Label>{t("field.limit")}</Label>
          <Input
            value={filters.limit}
            onChange={(value) => onFilterChange("limit", value)}
          />
        </div>
        <div>
          <Label>{t("field.from_at")}</Label>
          <Input
            value={filters.fromAt}
            onChange={(value) => onFilterChange("fromAt", value)}
            placeholder={t("common.datetimePlaceholder")}
          />
        </div>
        <div>
          <Label>{t("field.to_at")}</Label>
          <Input
            value={filters.toAt}
            onChange={(value) => onFilterChange("toAt", value)}
            placeholder={t("common.datetimePlaceholder")}
          />
        </div>
      </div>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
        <Button onClick={onRunQuery} disabled={busy}>
          {loadingRows || loadingCount ? t("common.loading") : t("common.query")}
        </Button>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <span className="text-xs text-muted">
            {t("requests.clear.selectedCount", { count: selectedCount })}
          </span>
          <Button
            variant="neutral"
            disabled={selectedCount === 0 || clearingPayload || deletingLogs}
            onClick={() => onClearPayload(false)}
          >
            {t("requests.clear.selected")}
          </Button>
          <Button
            variant="neutral"
            disabled={clearingPayload || deletingLogs}
            onClick={() => onClearPayload(true)}
          >
            {t("requests.clear.all")}
          </Button>
          <Button
            variant="danger"
            disabled={selectedCount === 0 || clearingPayload || deletingLogs}
            onClick={() => onDeleteLogs(false)}
          >
            {t("requests.delete.selected")}
          </Button>
          <Button
            variant="danger"
            disabled={clearingPayload || deletingLogs}
            onClick={() => onDeleteLogs(true)}
          >
            {t("requests.delete.all")}
          </Button>
        </div>
      </div>
    </>
  );
}
