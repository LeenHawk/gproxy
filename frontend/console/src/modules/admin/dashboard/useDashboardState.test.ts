// @vitest-environment jsdom

import { describe, expect, it } from "vitest";
import { act } from "react";
import React from "react";
import { createRoot } from "react-dom/client";

import {
  buildDashboardQuery,
  buildPresetRange,
  defaultDashboardRange,
  resolveBucketSeconds,
  useDashboardState,
  validateCustomRange,
} from "./useDashboardState";

describe("dashboard state helpers", () => {
  it("defaults to a 24h preset with hourly buckets", () => {
    const nowMs = Date.UTC(2026, 3, 15, 12, 0, 0);
    const range = defaultDashboardRange(nowMs);

    expect(range.kind).toBe("preset");
    expect(range.preset).toBe("24h");
    expect(range.fromUnixMs).toBe(nowMs - 24 * 60 * 60 * 1000);
    expect(range.toUnixMs).toBe(nowMs);
    expect(buildDashboardQuery(range)).toEqual({
      from_unix_ms: nowMs - 24 * 60 * 60 * 1000,
      to_unix_ms: nowMs,
      bucket_seconds: 3600,
    });
  });

  it("derives bucket widths from the selected span", () => {
    expect(resolveBucketSeconds(30 * 60 * 1000)).toBe(60);
    expect(resolveBucketSeconds(6 * 60 * 60 * 1000)).toBe(3600);
    expect(resolveBucketSeconds(10 * 24 * 60 * 60 * 1000)).toBe(3600);
    expect(resolveBucketSeconds(45 * 24 * 60 * 60 * 1000)).toBe(86400);
  });

  it("builds preset ranges from a fixed clock", () => {
    const nowMs = Date.UTC(2026, 3, 15, 12, 0, 0);
    const range = buildPresetRange("7d", nowMs);

    expect(range.fromUnixMs).toBe(nowMs - 7 * 24 * 60 * 60 * 1000);
    expect(range.toUnixMs).toBe(nowMs);
    expect(buildDashboardQuery(range).bucket_seconds).toBe(3600);
  });

  it("validates custom ranges before querying", () => {
    const fromUnixMs = Date.UTC(2026, 3, 15, 14, 0, 0);
    const toUnixMs = Date.UTC(2026, 3, 15, 12, 0, 0);

    expect(validateCustomRange(fromUnixMs, toUnixMs)).toMatch(/before/i);
    expect(validateCustomRange(toUnixMs, fromUnixMs)).toBeNull();
  });

  it("fetches dashboard data once on mount without looping", async () => {
    const responses = [
      {
        kpi: {
          total_requests: 1,
          success_count: 1,
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
      },
      { rows: [] },
      { rows: [] },
      [],
    ];
    let fetchCalls = 0;
    const originalFetch = globalThis.fetch;
    globalThis.fetch = (async () => {
      const payload = responses[fetchCalls] ?? responses[responses.length - 1];
      fetchCalls += 1;
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }) as typeof fetch;

    function Harness() {
      useDashboardState("token");
      return null;
    }

    const container = document.createElement("div");
    const root = createRoot(container);
    try {
      await act(async () => {
        root.render(React.createElement(Harness));
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(fetchCalls).toBe(4);
    } finally {
      await act(async () => {
        root.unmount();
      });
      globalThis.fetch = originalFetch;
    }
  });
});
