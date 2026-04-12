import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { useI18n } from "../../app/i18n";
import {
  Button,
  Card,
  Input,
  Label,
  MetricCard,
  SearchableSelect,
  Select,
  Table,
} from "../../components/ui";
import { apiJson } from "../../lib/api";
import { authHeaders } from "../../lib/auth";
import {
  formatAtForViewer,
  parseAtToUnixMs,
  parseDateTimeLocalToUnixMs,
} from "../../lib/datetime";
import { parseOptionalI64 } from "../../lib/form";
import { scopeAll, scopeEq } from "../../lib/scope";
import type { UsageQueryRow, UsageSummary } from "../../lib/types/shared";

type UsageQuerySnapshot = {
  channel: string;
  model: string;
  userKeyId: number | null;
  fromUnixMs: number | null;
  toUnixMs: number | null;
  maxRows: number | null;
};

type UsagePageCursor = {
  atUnixMs: number;
  traceId: number;
};

type MyKeyRow = {
  id: number;
  api_key: string;
  label?: string | null;
  enabled: boolean;
};

function emptySummary(): UsageSummary {
  return {
    count: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_read_input_tokens: 0,
    cache_creation_input_tokens: 0,
    cache_creation_input_tokens_5min: 0,
    cache_creation_input_tokens_1h: 0,
    total_cost: 0,
  };
}

/// Render a quota cost value as a compact decimal. Mirrors the helper
/// in `admin/UsageModule.tsx`; kept inline so this module stays
/// dependency-free of the admin tree.
function formatCost(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return "";
  }
  if (value === 0) {
    return "0";
  }
  return value
    .toFixed(4)
    .replace(/\.?0+$/, "");
}

function defaultPageSizeByViewport(): number {
  if (typeof window === "undefined") {
    return 20;
  }
  if (window.innerWidth < 640) {
    return 5;
  }
  if (window.innerWidth < 1024) {
    return 10;
  }
  if (window.innerWidth < 1600) {
    return 20;
  }
  return 50;
}

function toPositiveOrNull(value: number | null): number | null {
  if (value === null || value <= 0) {
    return null;
  }
  return value;
}

function buildUsageBasePayload(snapshot: UsageQuerySnapshot) {
  return {
    provider_id: scopeAll<number>(),
    credential_id: scopeAll<number>(),
    channel: snapshot.channel ? scopeEq(snapshot.channel) : scopeAll<string>(),
    model: snapshot.model ? scopeEq(snapshot.model) : scopeAll<string>(),
    // user_id is forcibly overridden on the backend to the session user —
    // sending "All" here is fine and keeps the client simple.
    user_id: scopeAll<number>(),
    user_key_id:
      snapshot.userKeyId === null ? scopeAll<number>() : scopeEq(snapshot.userKeyId),
    from_unix_ms: snapshot.fromUnixMs,
    to_unix_ms: snapshot.toUnixMs,
  };
}

function buildUsageRowsPayload(
  snapshot: UsageQuerySnapshot,
  options: { limit: number; cursor: UsagePageCursor | null },
) {
  return {
    ...buildUsageBasePayload(snapshot),
    cursor_at_unix_ms: options.cursor?.atUnixMs ?? null,
    cursor_trace_id: options.cursor?.traceId ?? null,
    limit: options.limit,
  };
}

function usageCursorFromRows(rows: UsageQueryRow[]): UsagePageCursor | null {
  const last = rows.length > 0 ? rows[rows.length - 1] : undefined;
  if (!last) {
    return null;
  }
  const atUnixMs = parseAtToUnixMs(last.at);
  if (atUnixMs === null) {
    return null;
  }
  return { atUnixMs, traceId: last.trace_id };
}

export function MyUsageModule({
  sessionToken,
  notify,
}: {
  sessionToken: string;
  notify: (kind: "success" | "error" | "info", message: string) => void;
}) {
  const { t } = useI18n();
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const [filters, setFilters] = useState({
    channel: "",
    model: "",
    userKeyId: "",
    fromAt: "",
    toAt: "",
    limit: "200",
  });
  const [rows, setRows] = useState<UsageQueryRow[]>([]);
  const [summary, setSummary] = useState<UsageSummary>(emptySummary);
  const [totalRows, setTotalRows] = useState(0);
  const [pageSize, setPageSize] = useState<number>(() => defaultPageSizeByViewport());
  const [page, setPage] = useState(1);
  const [pageCursors, setPageCursors] = useState<Array<UsagePageCursor | null>>([null]);
  const [activeQuery, setActiveQuery] = useState<UsageQuerySnapshot | null>(null);
  const [loadingRows, setLoadingRows] = useState(false);
  const [loadingMeta, setLoadingMeta] = useState(false);
  const [userKeys, setUserKeys] = useState<MyKeyRow[]>([]);
  const [knownChannels, setKnownChannels] = useState<string[]>([]);
  const [knownModels, setKnownModels] = useState<string[]>([]);
  const [knownModelsByChannel, setKnownModelsByChannel] = useState<Record<string, string[]>>({});

  // Split into two refs so the summary and rows requests don't clobber
  // each other's cancellation token. Sharing one ref lets the second
  // effect bump the counter before the first request resolves, which
  // leaves `loadingMeta` stuck at `true` forever (button pinned on
  // "querying").
  const summaryTokenRef = useRef(0);
  const rowsTokenRef = useRef(0);

  /// Harvest observed channels/models from loaded rows so the filter
  /// dropdowns grow with the visible data. Mirrors the admin UsageModule.
  const collectUsageMetadata = useCallback((usageRows: UsageQueryRow[]) => {
    const channels = usageRows
      .map((row) => row.provider_channel?.trim() ?? "")
      .filter((value) => value.length > 0);
    const models = usageRows
      .map((row) => row.model?.trim() ?? "")
      .filter((value) => value.length > 0);
    const pairs = usageRows
      .map((row) => ({
        channel: row.provider_channel?.trim() ?? "",
        model: row.model?.trim() ?? "",
      }))
      .filter((item) => item.channel.length > 0 && item.model.length > 0);

    if (channels.length > 0) {
      setKnownChannels((prev) => Array.from(new Set([...prev, ...channels])).sort());
    }
    if (models.length > 0) {
      setKnownModels((prev) => Array.from(new Set([...prev, ...models])).sort());
    }
    if (pairs.length > 0) {
      setKnownModelsByChannel((prev) => {
        const merged = new Map<string, Set<string>>();
        for (const [channel, channelModels] of Object.entries(prev)) {
          merged.set(channel, new Set(channelModels));
        }
        for (const item of pairs) {
          const bucket = merged.get(item.channel);
          if (bucket) {
            bucket.add(item.model);
          } else {
            merged.set(item.channel, new Set([item.model]));
          }
        }
        const next: Record<string, string[]> = {};
        for (const [channel, modelSet] of merged.entries()) {
          next[channel] = Array.from(modelSet).sort();
        }
        return next;
      });
    }
  }, []);

  // Fetch the user's keys + seed the channel/model dropdowns from a recent
  // rows sample so users don't have to run a query to see the options.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [keys, seed] = await Promise.all([
          apiJson<MyKeyRow[]>("/user/keys/query", {
            method: "POST",
            headers,
            body: JSON.stringify({}),
          }),
          apiJson<UsageQueryRow[]>("/user/usages/query", {
            method: "POST",
            headers,
            body: JSON.stringify({
              provider_id: scopeAll<number>(),
              credential_id: scopeAll<number>(),
              channel: scopeAll<string>(),
              model: scopeAll<string>(),
              user_id: scopeAll<number>(),
              user_key_id: scopeAll<number>(),
              limit: 1000,
            }),
          }),
        ]);
        if (cancelled) {
          return;
        }
        setUserKeys([...keys].sort((a, b) => a.id - b.id));
        collectUsageMetadata(seed);
      } catch (error) {
        if (!cancelled) {
          notify("error", error instanceof Error ? error.message : String(error));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [collectUsageMetadata, headers, notify]);

  const selectedChannel = filters.channel.trim();

  const userKeyOptions = useMemo(
    () => [
      { value: "", label: t("common.all") },
      ...userKeys.map((row) => {
        const key = row.api_key.trim();
        const preview = key.length <= 14 ? key : `${key.slice(0, 6)}...${key.slice(-4)}`;
        return {
          value: String(row.id),
          label: `#${row.id} · ${preview}`,
        };
      }),
    ],
    [t, userKeys],
  );

  const channelOptions = useMemo(
    () => [
      { value: "", label: t("common.all") },
      ...knownChannels.map((value) => ({ value, label: value })),
    ],
    [knownChannels, t],
  );

  const modelOptions = useMemo(() => {
    const scoped =
      selectedChannel.length > 0 ? (knownModelsByChannel[selectedChannel] ?? []) : knownModels;
    return [
      { value: "", label: t("common.all") },
      ...scoped.map((value) => ({ value, label: value })),
    ];
  }, [knownModels, knownModelsByChannel, selectedChannel, t]);

  useEffect(() => {
    if (!filters.model) {
      return;
    }
    const exists = modelOptions.some((item) => item.value === filters.model);
    if (!exists) {
      setFilters((prev) => ({ ...prev, model: "" }));
    }
  }, [filters.model, modelOptions]);

  useEffect(() => {
    setPage(1);
    setPageCursors([null]);
  }, [pageSize, activeQuery]);

  const totalPages = Math.max(1, Math.ceil(totalRows / pageSize));
  const currentPageCursor = pageCursors[page - 1] ?? null;

  useEffect(() => {
    if (page > totalPages) {
      setPage(totalPages);
    }
  }, [page, totalPages]);

  const buildSnapshot = useCallback((): UsageQuerySnapshot => {
    const userKeyId = parseOptionalI64(filters.userKeyId);
    const fromUnixMs = parseDateTimeLocalToUnixMs(filters.fromAt);
    const toUnixMs = parseDateTimeLocalToUnixMs(filters.toAt);
    return {
      channel: filters.channel.trim(),
      model: filters.model.trim(),
      userKeyId,
      fromUnixMs,
      toUnixMs,
      maxRows: toPositiveOrNull(parseOptionalI64(filters.limit)),
    };
  }, [filters]);

  const runQuery = useCallback(() => {
    const snapshot = buildSnapshot();
    setActiveQuery(snapshot);
    setPage(1);
    setPageCursors([null]);
    setRows([]);
    setTotalRows(0);
  }, [buildSnapshot]);

  // Load the summary aggregate for the active query. This is the fix for the
  // previous "visible-page-only" totals bug: the backend now sums across the
  // entire filtered set, not just the current 50-row slice.
  useEffect(() => {
    if (!activeQuery) {
      return;
    }
    const token = ++summaryTokenRef.current;
    setLoadingMeta(true);
    void apiJson<UsageSummary>("/user/usages/summary", {
      method: "POST",
      headers,
      body: JSON.stringify(buildUsageBasePayload(activeQuery)),
    })
      .then((result) => {
        if (summaryTokenRef.current !== token) {
          return;
        }
        const maxRows = activeQuery.maxRows;
        setTotalRows(maxRows === null ? result.count : Math.min(result.count, maxRows));
        setSummary(result);
      })
      .catch((error) => {
        if (summaryTokenRef.current !== token) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (summaryTokenRef.current === token) {
          setLoadingMeta(false);
        }
      });
  }, [activeQuery, headers, notify]);

  // Load the current page of rows via cursor pagination.
  useEffect(() => {
    if (!activeQuery) {
      setRows([]);
      return;
    }
    const offset = (page - 1) * pageSize;
    const maxRows = activeQuery.maxRows;
    const remaining = maxRows === null ? pageSize : Math.max(0, maxRows - offset);
    const limit = Math.min(pageSize, remaining || pageSize);
    if (limit <= 0) {
      setRows([]);
      return;
    }

    const token = ++rowsTokenRef.current;
    setLoadingRows(true);
    void apiJson<UsageQueryRow[]>("/user/usages/query", {
      method: "POST",
      headers,
      body: JSON.stringify(
        buildUsageRowsPayload(activeQuery, { limit, cursor: currentPageCursor }),
      ),
    })
      .then((data) => {
        if (rowsTokenRef.current !== token) {
          return;
        }
        setRows(data);
        collectUsageMetadata(data);
        const nextCursor = usageCursorFromRows(data);
        if (nextCursor && data.length === limit) {
          setPageCursors((prev) => {
            if (prev[page] && prev[page]?.traceId === nextCursor.traceId) {
              return prev;
            }
            const next = prev.slice(0, page);
            while (next.length <= page) {
              next.push(null);
            }
            next[page] = nextCursor;
            return next;
          });
        }
      })
      .catch((error) => {
        if (rowsTokenRef.current !== token) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (rowsTokenRef.current === token) {
          setLoadingRows(false);
        }
      });
  }, [activeQuery, collectUsageMetadata, currentPageCursor, headers, notify, page, pageSize]);

  const canGoNext = page < totalPages && pageCursors[page] !== undefined;

  const tableColumns = [
    t("table.trace_id"),
    t("table.provider"),
    t("table.model"),
    t("table.input"),
    t("table.output"),
    t("table.cache_read"),
    t("table.cache_creation"),
    t("table.cache_creation_5m"),
    t("table.cache_creation_1h"),
    t("table.cost"),
    t("table.at"),
  ];

  return (
    <div className="space-y-4">
      <Card title={t("myUsage.title")} subtitle={t("myUsage.subtitle")}>
        <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3">
          <div>
            <Label>{t("field.channel")}</Label>
            <SearchableSelect
              value={filters.channel}
              onChange={(v) => setFilters((p) => ({ ...p, channel: v }))}
              options={channelOptions}
              placeholder={t("common.all")}
              noResultLabel={t("common.none")}
            />
          </div>
          <div>
            <Label>{t("field.model")}</Label>
            <SearchableSelect
              value={filters.model}
              onChange={(v) => setFilters((p) => ({ ...p, model: v }))}
              options={modelOptions}
              placeholder={t("common.all")}
              noResultLabel={t("common.none")}
            />
          </div>
          <div>
            <Label>{t("field.user_key_id")}</Label>
            <SearchableSelect
              value={filters.userKeyId}
              onChange={(v) => setFilters((p) => ({ ...p, userKeyId: v }))}
              options={userKeyOptions}
              placeholder={t("common.all")}
              noResultLabel={t("common.none")}
            />
          </div>
          <div>
            <Label>{t("field.from_at")}</Label>
            <Input
              value={filters.fromAt}
              onChange={(v) => setFilters((p) => ({ ...p, fromAt: v }))}
              placeholder={t("common.datetimePlaceholder")}
            />
          </div>
          <div>
            <Label>{t("field.to_at")}</Label>
            <Input
              value={filters.toAt}
              onChange={(v) => setFilters((p) => ({ ...p, toAt: v }))}
              placeholder={t("common.datetimePlaceholder")}
            />
          </div>
          <div>
            <Label>{t("field.limit")}</Label>
            <Input
              value={filters.limit}
              onChange={(v) => setFilters((p) => ({ ...p, limit: v }))}
            />
          </div>
        </div>
        <div className="mt-3">
          <Button onClick={runQuery} disabled={loadingRows || loadingMeta}>
            {loadingRows || loadingMeta ? t("common.loading") : t("common.query")}
          </Button>
        </div>
      </Card>
      <div className="grid gap-3 grid-cols-1 sm:grid-cols-2 md:grid-cols-4 xl:grid-cols-8">
        <MetricCard label={t("metric.count")} value={summary.count} />
        <MetricCard label={t("metric.input_tokens")} value={summary.input_tokens} />
        <MetricCard label={t("metric.output_tokens")} value={summary.output_tokens} />
        <MetricCard label={t("metric.cache_read")} value={summary.cache_read_input_tokens} />
        <MetricCard
          label={t("metric.cache_creation")}
          value={summary.cache_creation_input_tokens}
        />
        <MetricCard
          label={t("metric.cache_creation_5m")}
          value={summary.cache_creation_input_tokens_5min}
        />
        <MetricCard
          label={t("metric.cache_creation_1h")}
          value={summary.cache_creation_input_tokens_1h}
        />
        <MetricCard label={t("metric.cost")} value={formatCost(summary.total_cost)} />
      </div>
      <Card title={t("myUsage.rows")}>
        <Table
          columns={tableColumns}
          rows={rows.map((row) => ({
            [tableColumns[0]]: row.downstream_trace_id ?? row.trace_id,
            [tableColumns[1]]: row.provider_channel ?? "",
            [tableColumns[2]]: row.model ?? "",
            [tableColumns[3]]: row.input_tokens ?? "",
            [tableColumns[4]]: row.output_tokens ?? "",
            [tableColumns[5]]: row.cache_read_input_tokens ?? "",
            [tableColumns[6]]: row.cache_creation_input_tokens ?? "",
            [tableColumns[7]]: row.cache_creation_input_tokens_5min ?? "",
            [tableColumns[8]]: row.cache_creation_input_tokens_1h ?? "",
            [tableColumns[9]]: formatCost(row.cost),
            [tableColumns[10]]: formatAtForViewer(row.at),
          }))}
        />
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-muted">
          <div>{t("common.pager.stats", { shown: rows.length, total: totalRows })}</div>
          <div className="flex items-center gap-2">
            <span>{t("common.show")}</span>
            <div className="w-20">
              <Select
                value={String(pageSize)}
                onChange={(value) => setPageSize(Number(value))}
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
              onClick={() => setPage((prev) => Math.max(1, prev - 1))}
            >
              {t("common.pager.prev")}
            </Button>
            <span>{t("common.pager.page", { current: page, total: totalPages })}</span>
            <Button
              variant="neutral"
              disabled={!canGoNext || loadingRows || loadingMeta}
              onClick={() => setPage((prev) => Math.min(totalPages, prev + 1))}
            >
              {t("common.pager.next")}
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}
