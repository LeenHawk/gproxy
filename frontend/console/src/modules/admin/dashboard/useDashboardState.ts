import {
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
} from "react";

import { apiJson } from "../../../lib/api";
import { authHeaders } from "../../../lib/auth";
import { parseDateTimeLocalToUnixMs } from "../../../lib/datetime";
import type {
  CredentialHealthRow,
  DashboardOverview,
  DashboardQuery,
  DashboardTopModels,
  DashboardTopProviders,
} from "../../../lib/types/admin";
import type { DashboardBundle, DashboardDataState, DashboardPreset, DashboardRange } from "./types";

const HOUR_MS = 60 * 60 * 1000;
const DAY_MS = 24 * HOUR_MS;

function emptyOverview(): DashboardOverview {
  return {
    kpi: {
      total_requests: 0,
      success_count: 0,
      error_4xx_count: 0,
      error_5xx_count: 0,
      total_cost: 0,
      total_input_tokens: 0,
      total_output_tokens: 0,
      avg_latency_ms: null,
      max_latency_ms: null,
    },
    traffic: [],
    status_codes: [],
  };
}

function emptyTopProviders(): DashboardTopProviders {
  return { rows: [] };
}

function emptyTopModels(): DashboardTopModels {
  return { rows: [] };
}

export function resolveBucketSeconds(spanMs: number): number {
  if (spanMs < 2 * HOUR_MS) {
    return 60;
  }
  if (spanMs < 14 * DAY_MS) {
    return 3600;
  }
  return 86400;
}

export function buildPresetRange(preset: DashboardPreset, nowMs = Date.now()): DashboardRange {
  const spanMs =
    preset === "1h" ? HOUR_MS : preset === "24h" ? DAY_MS : preset === "7d" ? 7 * DAY_MS : 30 * DAY_MS;
  return {
    kind: "preset",
    preset,
    fromUnixMs: nowMs - spanMs,
    toUnixMs: nowMs,
  };
}

export function defaultDashboardRange(nowMs = Date.now()): DashboardRange {
  return buildPresetRange("24h", nowMs);
}

export function buildDashboardQuery(range: DashboardRange): DashboardQuery {
  return {
    from_unix_ms: range.fromUnixMs,
    to_unix_ms: range.toUnixMs,
    bucket_seconds: resolveBucketSeconds(range.toUnixMs - range.fromUnixMs),
  };
}

export function validateCustomRange(fromUnixMs: number | null, toUnixMs: number | null): string | null {
  if (fromUnixMs === null || toUnixMs === null) {
    return "Custom range requires both start and end.";
  }
  if (fromUnixMs >= toUnixMs) {
    return "Start time must be before end time.";
  }
  return null;
}

function formatDateTimeLocal(unixMs: number): string {
  const date = new Date(unixMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

async function fetchDashboardBundle(
  headers: Headers,
  range: DashboardRange,
): Promise<DashboardBundle> {
  const query = buildDashboardQuery(range);
  const [overview, topProviders, topModels, credentialHealth] = await Promise.all([
    apiJson<DashboardOverview>("/admin/dashboard/overview", {
      method: "POST",
      headers,
      body: JSON.stringify(query),
    }),
    apiJson<DashboardTopProviders>("/admin/dashboard/top-providers", {
      method: "POST",
      headers,
      body: JSON.stringify(query),
    }),
    apiJson<DashboardTopModels>("/admin/dashboard/top-models", {
      method: "POST",
      headers,
      body: JSON.stringify(query),
    }),
    apiJson<CredentialHealthRow[]>("/admin/credential-statuses/query", {
      method: "POST",
      headers,
      body: JSON.stringify({}),
    }),
  ]);
  return { overview, topProviders, topModels, credentialHealth };
}

export function useDashboardState(sessionToken: string) {
  const headers = useMemo(() => authHeaders(sessionToken), [sessionToken]);
  const initialRange = useMemo(() => defaultDashboardRange(), []);

  const [range, setRange] = useState<DashboardRange>(initialRange);
  const [customFrom, setCustomFrom] = useState(() => formatDateTimeLocal(initialRange.fromUnixMs));
  const [customTo, setCustomTo] = useState(() => formatDateTimeLocal(initialRange.toUnixMs));
  const [customError, setCustomError] = useState<string | null>(null);
  const [autoRefreshMs, setAutoRefreshMs] = useState(0);

  const [overview, setOverview] = useState<DashboardDataState<DashboardOverview>>({
    data: emptyOverview(),
    loading: true,
    error: null,
  });
  const [topProviders, setTopProviders] = useState<DashboardDataState<DashboardTopProviders>>({
    data: emptyTopProviders(),
    loading: true,
    error: null,
  });
  const [topModels, setTopModels] = useState<DashboardDataState<DashboardTopModels>>({
    data: emptyTopModels(),
    loading: true,
    error: null,
  });
  const [credentialHealth, setCredentialHealth] = useState<DashboardDataState<CredentialHealthRow[]>>({
    data: [],
    loading: true,
    error: null,
  });

  const loadingRef = useRef(false);

  const refresh = useEffectEvent(async () => {
    if (loadingRef.current) {
      return;
    }
    loadingRef.current = true;
    setOverview((current) => ({ ...current, loading: true, error: null }));
    setTopProviders((current) => ({ ...current, loading: true, error: null }));
    setTopModels((current) => ({ ...current, loading: true, error: null }));
    setCredentialHealth((current) => ({ ...current, loading: true, error: null }));

    try {
      const bundle = await Promise.allSettled([
        apiJson<DashboardOverview>("/admin/dashboard/overview", {
          method: "POST",
          headers,
          body: JSON.stringify(buildDashboardQuery(range)),
        }),
        apiJson<DashboardTopProviders>("/admin/dashboard/top-providers", {
          method: "POST",
          headers,
          body: JSON.stringify(buildDashboardQuery(range)),
        }),
        apiJson<DashboardTopModels>("/admin/dashboard/top-models", {
          method: "POST",
          headers,
          body: JSON.stringify(buildDashboardQuery(range)),
        }),
        apiJson<CredentialHealthRow[]>("/admin/credential-statuses/query", {
          method: "POST",
          headers,
          body: JSON.stringify({}),
        }),
      ]);

      const [overviewResult, providersResult, modelsResult, credentialResult] = bundle;

      setOverview({
        data: overviewResult.status === "fulfilled" ? overviewResult.value : emptyOverview(),
        loading: false,
        error: overviewResult.status === "rejected" ? String(overviewResult.reason) : null,
      });
      setTopProviders({
        data: providersResult.status === "fulfilled" ? providersResult.value : emptyTopProviders(),
        loading: false,
        error: providersResult.status === "rejected" ? String(providersResult.reason) : null,
      });
      setTopModels({
        data: modelsResult.status === "fulfilled" ? modelsResult.value : emptyTopModels(),
        loading: false,
        error: modelsResult.status === "rejected" ? String(modelsResult.reason) : null,
      });
      setCredentialHealth({
        data: credentialResult.status === "fulfilled" ? credentialResult.value : [],
        loading: false,
        error: credentialResult.status === "rejected" ? String(credentialResult.reason) : null,
      });
    } finally {
      loadingRef.current = false;
    }
  });

  useEffect(() => {
    void refresh();
  }, [range.fromUnixMs, range.toUnixMs, range.kind, range.preset, headers]);

  useEffect(() => {
    if (autoRefreshMs <= 0) {
      return;
    }
    const timer = window.setInterval(() => {
      void refresh();
    }, autoRefreshMs);
    return () => window.clearInterval(timer);
  }, [autoRefreshMs]);

  const selectPreset = (preset: DashboardPreset) => {
    const next = buildPresetRange(preset);
    setRange(next);
    setCustomFrom(formatDateTimeLocal(next.fromUnixMs));
    setCustomTo(formatDateTimeLocal(next.toUnixMs));
    setCustomError(null);
  };

  const applyCustomRange = () => {
    const fromUnixMs = parseDateTimeLocalToUnixMs(customFrom);
    const toUnixMs = parseDateTimeLocalToUnixMs(customTo);
    const error = validateCustomRange(fromUnixMs, toUnixMs);
    setCustomError(error);
    if (error || fromUnixMs === null || toUnixMs === null) {
      return;
    }
    setRange({
      kind: "custom",
      preset: null,
      fromUnixMs,
      toUnixMs,
    });
  };

  return {
    range,
    customFrom,
    customTo,
    customError,
    autoRefreshMs,
    overview,
    topProviders,
    topModels,
    credentialHealth,
    setCustomFrom,
    setCustomTo,
    setAutoRefreshMs,
    selectPreset,
    applyCustomRange,
    refresh,
  };
}

export { fetchDashboardBundle };
