import { describe, expect, it } from "vitest";
import {
  MAX_REANCHOR_AFTER_SECS, assembleAffinitySettings, initialAffinity,
} from "./affinity-settings";

describe("route affinity settings", () => {
  it("keeps the legacy user subject implicit", () => {
    const state = initialAffinity({ affinity: { enabled: true } });
    expect(state).toEqual({ enabled: true, subject: "user", reanchorAfterSecs: "" });
    expect(assembleAffinitySettings({}, state)).toEqual({
      ok: true,
      settings: { affinity: { enabled: true } },
    });
  });

  it("writes conversation affinity without dropping unknown settings", () => {
    const base = {
      public_namespace: "openai",
      future_route_option: { enabled: true },
      affinity: {
        enabled: true,
        subject: "conversation",
        reanchor_after_secs: 600,
        future_affinity_option: "keep",
      },
    };
    const state = initialAffinity(base);
    expect(state).toEqual({
      enabled: true,
      subject: "conversation",
      reanchorAfterSecs: "600",
    });
    state.reanchorAfterSecs = String(MAX_REANCHOR_AFTER_SECS);

    expect(assembleAffinitySettings(base, state)).toEqual({
      ok: true,
      settings: {
        public_namespace: "openai",
        future_route_option: { enabled: true },
        affinity: {
          enabled: true,
          subject: "conversation",
          reanchor_after_secs: MAX_REANCHOR_AFTER_SECS,
          future_affinity_option: "keep",
        },
      },
    });
  });

  it("removes owned affinity fields when disabled but retains unknown fields", () => {
    const base = {
      affinity: {
        enabled: true,
        subject: "conversation",
        reanchor_after_secs: 600,
        future_affinity_option: "keep",
      },
    };
    expect(assembleAffinitySettings(base, {
      enabled: false,
      subject: "conversation",
      reanchorAfterSecs: "600",
    })).toEqual({
      ok: true,
      settings: { affinity: { future_affinity_option: "keep" } },
    });
  });

  it("rejects invalid re-anchor intervals", () => {
    for (const reanchorAfterSecs of [
      "0", "1.5", "not-a-number", String(MAX_REANCHOR_AFTER_SECS + 1),
    ]) {
      expect(assembleAffinitySettings({}, {
        enabled: true,
        subject: "user",
        reanchorAfterSecs,
      })).toEqual({ ok: false, error: "reanchor_after_secs_invalid" });
    }
  });
});
