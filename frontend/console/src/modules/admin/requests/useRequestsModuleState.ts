import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { apiJson } from "../../../lib/api";
import { authHeaders } from "../../../lib/auth";
import { parseAtToUnixMs, parseDateTimeLocalToUnixMs } from "../../../lib/datetime";
import { parseOptionalI64 } from "../../../lib/form";
import { scopeAll, scopeEq } from "../../../lib/scope";
import type {
  DownstreamRequestQueryRow,
  RequestClearAck,
  UpstreamRequestQueryRow,
} from "../../../lib/types/admin";
import type { CountResponse } from "../../../lib/types/shared";
import { useAdminFilterOptions } from "../hooks/useAdminFilterOptions";
import {
  KNOWN_DOWNSTREAM_REQUEST_PATHS,
  KNOWN_UPSTREAM_REQUEST_TARGETS,
} from "../requests-filter";
import type {
  NotifyFn,
  RequestBodyPayload,
  RequestKind,
  RequestQuerySnapshot,
  RequestRow,
  RequestsFilterState,
  SelectOption,
  TranslateFn,
} from "./types";

const DEFAULT_FILTERS: RequestsFilterState = {
  providerId: "",
  credentialId: "",
  userId: "",
  userKeyId: "",
  requestPathContains: "",
  fromAt: "",
  toAt: "",
  limit: "100",
};

type RequestPageCursor = {
  atUnixMs: number;
  traceId: number;
};

/// Pick a sensible default page size based on the viewport width so
/// mobile users don't get a huge table and desktop users don't have to
/// scroll through 5-row pages. Mirrors the sample gproxy behaviour.
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

function buildRequestCountPayload(snapshot: RequestQuerySnapshot) {
  if (snapshot.kind === "upstream") {
    return {
      trace_id: scopeAll<number>(),
      provider_id:
        snapshot.providerId === null ? scopeAll<number>() : scopeEq(snapshot.providerId),
      credential_id:
        snapshot.credentialId === null ? scopeAll<number>() : scopeEq(snapshot.credentialId),
      ...(snapshot.pathContains ? { request_url_contains: snapshot.pathContains } : {}),
      ...(snapshot.fromUnixMs !== null ? { from_unix_ms: snapshot.fromUnixMs } : {}),
      ...(snapshot.toUnixMs !== null ? { to_unix_ms: snapshot.toUnixMs } : {}),
    };
  }
  return {
    trace_id: scopeAll<number>(),
    user_id: snapshot.userId === null ? scopeAll<number>() : scopeEq(snapshot.userId),
    user_key_id: snapshot.userKeyId === null ? scopeAll<number>() : scopeEq(snapshot.userKeyId),
    ...(snapshot.pathContains ? { request_path_contains: snapshot.pathContains } : {}),
    ...(snapshot.fromUnixMs !== null ? { from_unix_ms: snapshot.fromUnixMs } : {}),
    ...(snapshot.toUnixMs !== null ? { to_unix_ms: snapshot.toUnixMs } : {}),
  };
}

function buildRequestRowsPayload(
  snapshot: RequestQuerySnapshot,
  options: {
    limit: number;
    includeBody: boolean;
    traceId?: number;
    cursor?: RequestPageCursor | null;
  },
) {
  const base = buildRequestCountPayload(snapshot);
  return {
    ...base,
    include_body: options.includeBody,
    ...(options.traceId !== undefined ? { trace_id: scopeEq(options.traceId) } : {}),
    ...(options.cursor
      ? {
          cursor_at_unix_ms: options.cursor.atUnixMs,
          cursor_trace_id: options.cursor.traceId,
        }
      : {}),
    limit: options.limit,
  };
}

function requestQueryPath(kind: RequestKind): string {
  return kind === "upstream" ? "/admin/requests/upstream/query" : "/admin/requests/downstream/query";
}

function requestCountPath(kind: RequestKind): string {
  return kind === "upstream" ? "/admin/requests/upstream/count" : "/admin/requests/downstream/count";
}

function requestClearPath(kind: RequestKind): string {
  return kind === "upstream" ? "/admin/requests/upstream/clear" : "/admin/requests/downstream/clear";
}

function requestDeletePath(kind: RequestKind): string {
  return kind === "upstream"
    ? "/admin/requests/upstream/batch-delete"
    : "/admin/requests/downstream/batch-delete";
}

function cursorFromRows(rows: RequestRow[]): RequestPageCursor | null {
  const last = rows.length > 0 ? rows[rows.length - 1] : undefined;
  if (!last) {
    return null;
  }
  const atMs = parseAtToUnixMs(last.at);
  if (atMs === null) {
    return null;
  }
  return { atUnixMs: atMs, traceId: last.trace_id };
}

/// Centralised state + IO for the admin Requests module.
///
/// Responsibilities:
/// - Holds the kind toggle, filter form, page cursor stack, active query
///   snapshot, rows, total count, and selected trace ids.
/// - Exposes loader effects that react to `activeQuery + page + pageSize`
///   changes so the user doesn't have to manually refresh on pagination.
/// - Loads request/response bodies on demand (`ensureBodyLoaded`) via a
///   single-row `trace_id = Eq(...)` query with `include_body = true`,
///   so the list load never ships the full payload blobs.
/// - Exposes `clearPayload(all)` and `deleteLogs(all)` backed by the new
///   `/admin/requests/{kind}/clear` and `/batch-delete` routes.
export function useRequestsModuleState({
  sessionToken,
  notify,
  t,
}: {
  sessionToken: string;
  notify: NotifyFn;
  t: TranslateFn;
}) {
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const filterOptions = useAdminFilterOptions({ headers, notify, t });

  const [kind, setKind] = useState<RequestKind>("downstream");
  const [filters, setFilters] = useState<RequestsFilterState>(DEFAULT_FILTERS);
  const [activeQuery, setActiveQuery] = useState<RequestQuerySnapshot | null>(null);
  const [rows, setRows] = useState<RequestRow[]>([]);
  const [totalRows, setTotalRows] = useState(0);
  const [pageSize, setPageSize] = useState<number>(() => defaultPageSizeByViewport());
  const [page, setPage] = useState(1);
  const [pageCursors, setPageCursors] = useState<Array<RequestPageCursor | null>>([null]);
  const [loadingRows, setLoadingRows] = useState(false);
  const [loadingCount, setLoadingCount] = useState(false);
  const [clearingPayload, setClearingPayload] = useState(false);
  const [deletingLogs, setDeletingLogs] = useState(false);
  const [selectedTraceIds, setSelectedTraceIds] = useState<number[]>([]);

  const [bodyByTraceId, setBodyByTraceId] = useState<Record<number, RequestBodyPayload>>({});
  const [bodyLoadingByTraceId, setBodyLoadingByTraceId] = useState<Record<number, boolean>>({});
  const [bodyErrorByTraceId, setBodyErrorByTraceId] = useState<Record<number, string>>({});

  const queryTokenRef = useRef(0);

  const updateFilter = useCallback(
    <K extends keyof RequestsFilterState>(key: K, value: RequestsFilterState[K]) => {
      setFilters((prev) => ({ ...prev, [key]: value }));
    },
    [],
  );

  const providerOptions = filterOptions.providerOptions;
  const userOptions = filterOptions.userOptions;
  const filteredCredentialOptions = useMemo<SelectOption[]>(
    () => filterOptions.credentialOptionsBuilder(filters.providerId),
    [filterOptions, filters.providerId],
  );
  const filteredUserKeyOptions = useMemo<SelectOption[]>(
    () => filterOptions.userKeyOptionsBuilder(filters.userId),
    [filterOptions, filters.userId],
  );
  const requestPathOptions = useMemo<SelectOption[]>(
    () =>
      (kind === "upstream" ? KNOWN_UPSTREAM_REQUEST_TARGETS : KNOWN_DOWNSTREAM_REQUEST_PATHS).map(
        (value) => ({ value, label: value }),
      ),
    [kind],
  );

  const runQuery = useCallback(() => {
    const providerId = filters.providerId ? parseOptionalI64(filters.providerId) : null;
    const credentialId = filters.credentialId ? parseOptionalI64(filters.credentialId) : null;
    const userId = filters.userId ? parseOptionalI64(filters.userId) : null;
    const userKeyId = filters.userKeyId ? parseOptionalI64(filters.userKeyId) : null;
    const maxRows = toPositiveOrNull(parseOptionalI64(filters.limit));
    const snapshot: RequestQuerySnapshot = {
      kind,
      providerId,
      credentialId,
      userId,
      userKeyId,
      pathContains: filters.requestPathContains.trim(),
      fromUnixMs: parseDateTimeLocalToUnixMs(filters.fromAt),
      toUnixMs: parseDateTimeLocalToUnixMs(filters.toAt),
      maxRows,
    };
    setPage(1);
    setPageCursors([null]);
    setSelectedTraceIds([]);
    setBodyByTraceId({});
    setBodyLoadingByTraceId({});
    setBodyErrorByTraceId({});
    setActiveQuery(snapshot);
  }, [filters, kind]);

  // Reset pagination state whenever the user switches tab or changes page
  // size — cursor-based paging is only valid for a given snapshot + size.
  useEffect(() => {
    setPage(1);
    setPageCursors([null]);
  }, [pageSize, activeQuery]);

  // Load the total count for the active query. Separate from row loading
  // so the count can lag behind the initial row fetch without blocking
  // the visible list.
  useEffect(() => {
    if (!activeQuery) {
      setTotalRows(0);
      return;
    }
    const token = ++queryTokenRef.current;
    setLoadingCount(true);
    void apiJson<CountResponse>(requestCountPath(activeQuery.kind), {
      method: "POST",
      headers,
      body: JSON.stringify(buildRequestCountPayload(activeQuery)),
    })
      .then((response) => {
        if (queryTokenRef.current !== token) {
          return;
        }
        const maxRows = activeQuery.maxRows;
        setTotalRows(
          maxRows === null ? response.count : Math.min(response.count, maxRows),
        );
      })
      .catch((error) => {
        if (queryTokenRef.current !== token) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (queryTokenRef.current === token) {
          setLoadingCount(false);
        }
      });
  }, [activeQuery, headers, notify]);

  // Load the current page. The `pageCursors` stack stores the cursor
  // for the head of every visited page so going backwards is free and
  // going forward reuses the tail of the previous page load.
  useEffect(() => {
    if (!activeQuery) {
      setRows([]);
      return;
    }
    const token = ++queryTokenRef.current;
    setLoadingRows(true);
    const cursor = pageCursors[page - 1] ?? null;
    const offset = cursor ? 0 : (page - 1) * pageSize;
    const maxRows = activeQuery.maxRows;
    const remaining = maxRows === null ? pageSize : Math.max(0, maxRows - (page - 1) * pageSize);
    const limit = Math.min(pageSize, remaining || pageSize);

    void apiJson<RequestRow[]>(requestQueryPath(activeQuery.kind), {
      method: "POST",
      headers,
      body: JSON.stringify({
        ...buildRequestRowsPayload(activeQuery, {
          limit,
          includeBody: false,
          cursor: cursor ?? undefined,
        }),
        ...(cursor ? {} : offset > 0 ? { offset } : {}),
      }),
    })
      .then((response) => {
        if (queryTokenRef.current !== token) {
          return;
        }
        setRows(response);
        const nextCursor = cursorFromRows(response);
        if (nextCursor && response.length === limit) {
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
        if (queryTokenRef.current !== token) {
          return;
        }
        notify("error", error instanceof Error ? error.message : String(error));
      })
      .finally(() => {
        if (queryTokenRef.current === token) {
          setLoadingRows(false);
        }
      });
  }, [activeQuery, headers, notify, page, pageSize]);

  const totalPages = Math.max(1, Math.ceil(totalRows / pageSize));
  const canGoNext = page < totalPages && pageCursors[page] !== undefined;

  const toggleTraceIdSelected = useCallback((traceId: number) => {
    setSelectedTraceIds((prev) =>
      prev.includes(traceId) ? prev.filter((id) => id !== traceId) : [...prev, traceId],
    );
  }, []);

  /// Fetch the single-row request/response body blobs for a given row and
  /// cache them in `bodyByTraceId`. Triggered by the payload cell's eye
  /// toggle so the list-level query can stay lightweight (include_body
  /// is only ever set on per-row fetches).
  const ensureBodyLoaded = useCallback(
    async (row: RequestRow): Promise<RequestBodyPayload | undefined> => {
      if (!activeQuery) {
        return undefined;
      }
      const traceId = row.trace_id;
      if (bodyByTraceId[traceId]) {
        return bodyByTraceId[traceId];
      }
      if (bodyLoadingByTraceId[traceId]) {
        return undefined;
      }
      setBodyLoadingByTraceId((prev) => ({ ...prev, [traceId]: true }));
      setBodyErrorByTraceId((prev) => {
        const next = { ...prev };
        delete next[traceId];
        return next;
      });
      try {
        const body = buildRequestRowsPayload(activeQuery, {
          limit: 1,
          includeBody: true,
          traceId,
        });
        const response = await apiJson<RequestRow[]>(requestQueryPath(activeQuery.kind), {
          method: "POST",
          headers,
          body: JSON.stringify(body),
        });
        const match = response.find((item) => item.trace_id === traceId);
        const payload: RequestBodyPayload = {
          request_body: (match as DownstreamRequestQueryRow | UpstreamRequestQueryRow | undefined)?.request_body ?? null,
          response_body: (match as DownstreamRequestQueryRow | UpstreamRequestQueryRow | undefined)?.response_body ?? null,
        };
        setBodyByTraceId((prev) => ({ ...prev, [traceId]: payload }));
        return payload;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setBodyErrorByTraceId((prev) => ({ ...prev, [traceId]: message }));
        return undefined;
      } finally {
        setBodyLoadingByTraceId((prev) => {
          const next = { ...prev };
          delete next[traceId];
          return next;
        });
      }
    },
    [activeQuery, bodyByTraceId, bodyLoadingByTraceId, headers],
  );

  const refresh = useCallback(() => {
    if (activeQuery) {
      setActiveQuery({ ...activeQuery });
    }
  }, [activeQuery]);

  const clearPayload = useCallback(
    async (all: boolean) => {
      const ids = Array.from(new Set(selectedTraceIds.filter((id) => id > 0))).sort(
        (a, b) => a - b,
      );
      if (!all && ids.length === 0) {
        notify("info", t("common.none"));
        return;
      }
      const confirmed = all
        ? window.confirm(t("requests.clear.confirmAll"))
        : window.confirm(t("requests.clear.confirmSelected", { count: ids.length }));
      if (!confirmed) {
        return;
      }
      setClearingPayload(true);
      try {
        const ack = await apiJson<RequestClearAck>(requestClearPath(kind), {
          method: "POST",
          headers,
          body: JSON.stringify({ all, trace_ids: all ? [] : ids }),
        });
        notify("success", t("requests.clear.done", { count: ack.cleared }));
        setSelectedTraceIds([]);
        setBodyByTraceId({});
        setBodyLoadingByTraceId({});
        setBodyErrorByTraceId({});
        refresh();
      } catch (error) {
        notify("error", error instanceof Error ? error.message : String(error));
      } finally {
        setClearingPayload(false);
      }
    },
    [headers, kind, notify, refresh, selectedTraceIds, t],
  );

  const deleteLogs = useCallback(
    async (all: boolean) => {
      const ids = Array.from(new Set(selectedTraceIds.filter((id) => id > 0))).sort(
        (a, b) => a - b,
      );
      if (!all && ids.length === 0) {
        notify("info", t("common.none"));
        return;
      }
      const confirmed = all
        ? window.confirm(t("requests.delete.confirmAll"))
        : window.confirm(t("requests.delete.confirmSelected", { count: ids.length }));
      if (!confirmed) {
        return;
      }
      setDeletingLogs(true);
      try {
        // batch-delete expects the raw trace_id list; "all" is expressed
        // by passing every row id currently matching the snapshot, so we
        // first drain the query and collect ids.
        let idsToDelete = ids;
        if (all && activeQuery) {
          const maxBatch = 1000;
          const drained = await apiJson<RequestRow[]>(requestQueryPath(activeQuery.kind), {
            method: "POST",
            headers,
            body: JSON.stringify(
              buildRequestRowsPayload(activeQuery, { limit: maxBatch, includeBody: false }),
            ),
          });
          idsToDelete = drained.map((row) => row.trace_id);
          if (idsToDelete.length === 0) {
            notify("info", t("common.none"));
            return;
          }
        }
        await apiJson<unknown>(requestDeletePath(kind), {
          method: "POST",
          headers,
          body: JSON.stringify(idsToDelete),
        });
        notify("success", t("requests.delete.done", { count: idsToDelete.length }));
        setSelectedTraceIds([]);
        refresh();
      } catch (error) {
        notify("error", error instanceof Error ? error.message : String(error));
      } finally {
        setDeletingLogs(false);
      }
    },
    [activeQuery, headers, kind, notify, refresh, selectedTraceIds, t],
  );

  return {
    kind,
    setKind,
    filters,
    updateFilter,
    rows,
    pageSize,
    setPageSize,
    page,
    setPage,
    totalRows,
    totalPages,
    canGoNext,
    loadingRows,
    loadingCount,
    clearingPayload,
    deletingLogs,
    selectedTraceIds,
    bodyByTraceId,
    bodyLoadingByTraceId,
    bodyErrorByTraceId,
    isFilterOptionsLoading: filterOptions.isLoading,
    providerOptions,
    filteredCredentialOptions,
    userOptions,
    filteredUserKeyOptions,
    requestPathOptions,
    runQuery,
    ensureBodyLoaded,
    toggleTraceIdSelected,
    clearPayload,
    deleteLogs,
  };
}
